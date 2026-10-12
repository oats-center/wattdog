---
title: "Query the data"
description: "Read wattdog's Parquet history with DuckDB, tune thresholds from it, and set up retention."
---

wattdog writes every PowerMon advertisement it hears to Parquet: one row per advertisement, with
the reading, the device's identity, and the signal strength. That history is the record of what
your power system did, even while the network was down. Metrics only show the latest value.

## Where the files are

```text
/var/lib/wattdog/samples/date=2026-10-12/hour=14/part-20261012T140000Z.parquet           # hourly
/var/lib/wattdog/samples/date=2026-10-12/part-20261012.parquet                            # daily
```

The active file ends in `.parquet.inprogress`. wattdog renames it to `.parquet` only after
closing it cleanly, at a roll boundary or on shutdown. **Readers should ignore `.inprogress`
files**: they aren't valid Parquet until they are closed. The `*.parquet` globs below skip them.

See [Parquet layout](/docs/reference/parquet/) for every column.

## Voltage by the minute

```sql
SELECT
  date_trunc('minute', observed_at_utc) AS minute,
  serial,
  avg(voltage1_volts) AS avg_v1,
  avg(current_amps)   AS avg_current,
  avg(power_watts)    AS avg_power
FROM read_parquet('/var/lib/wattdog/samples/**/*.parquet')
GROUP BY minute, serial
ORDER BY minute, serial;
```

## Check a threshold before you trust it

How often would an `off` rule of "≤ 11.2 V for 2 minutes" have fired last week? This query finds
each run of readings at or below 11.2 V and reports the ones that lasted 2 minutes or more.

```sql
WITH s AS (
  SELECT observed_at_utc AS t, voltage1_volts <= 11.2 AS low
  FROM read_parquet('/var/lib/wattdog/samples/**/*.parquet')
  WHERE serial = 12345678
    AND observed_at_utc >= now() - INTERVAL 7 DAY
),
runs AS (
  SELECT *, sum(CASE WHEN low THEN 0 ELSE 1 END) OVER (ORDER BY t) AS grp FROM s
)
SELECT min(t) AS started, max(t) - min(t) AS lasted
FROM runs
WHERE low
GROUP BY grp
HAVING max(t) - min(t) >= INTERVAL 2 MINUTE
ORDER BY started;
```

This approximates wattdog's rule. It doesn't model stale gaps or `hold_for`; see [How thresholds
work](/docs/concepts/thresholds/) for the exact rules.

## Retention

wattdog never deletes files, so retention is up to you.

- The repository's `packaging/tmpfiles.d-wattdog.conf` keeps samples for 14 days through
  `systemd-tmpfiles --clean`. Check that cleanup is enabled on your image before relying on it.
- Any other cleanup should delete only completed `*.parquet` files.
- Don't point logrotate at the samples directory. Rotating or truncating an active file destroys
  it.
