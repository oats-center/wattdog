---
title: "Parquet layout"
description: "Where wattdog writes samples, how files roll, and every column in the schema."
---

## Paths

Files live under `[data].dir/samples`, partitioned Hive-style by UTC date (and hour, in hourly
mode):

```text
# roll = "hourly"
samples/date=YYYY-MM-DD/hour=HH/part-YYYYMMDDTHH0000Z.parquet

# roll = "daily"
samples/date=YYYY-MM-DD/part-YYYYMMDD.parquet
```

## Lifecycle

- The file being written ends in `.parquet.inprogress`. It isn't valid Parquet until it's closed,
  so readers must skip it.
- wattdog buffers rows and writes a batch every 4,096 rows or every 10 seconds, whichever comes
  first.
- At each roll boundary, and on SIGINT or SIGTERM, wattdog closes the file and renames it to
  `.parquet`.
- wattdog never deletes files. See [Retention](/docs/guides/querying-data/#Retention).

## Schema

One row per PowerMon advertisement. Nullable columns are empty when the device didn't report a
value or reported a sentinel value.

| Column | Type | Nullable | Notes |
|---|---|---|---|
| `schema_version` | int32 | | Schema version of this row. |
| `observed_at_unix_ms` | int64 | | When wattdog received it, in Unix milliseconds. |
| `observed_at_utc` | timestamp(ms, UTC) | | The same instant as a timestamp. |
| `serial` | uint64 | | PowerMon serial. |
| `address_raw` | uint64 | | Bluetooth address as a number. |
| `address_display` | string | yes | Bluetooth address, `AA:BB:…`. |
| `address_kind` | string | yes | Bluetooth address type. |
| `name` | string | yes | Device name. |
| `model` | string | yes | Device model. |
| `firmware_version_bcd` | uint16 | | Firmware version, raw BCD. |
| `firmware_version` | string | yes | Firmware version, decoded. |
| `hardware_revision_bcd` | uint8 | | Hardware revision, raw BCD. |
| `hardware_revision` | string | yes | Hardware revision, decoded. |
| `device_time_raw` | uint32 | | The device's own clock, raw. |
| `flags_raw` | uint32 | | Advertisement flags, raw. |
| `voltage1_volts` | float32 | | Voltage channel 1. |
| `voltage2_volts` | float32 | | Voltage channel 2. |
| `current_amps` | float32 | | Current. |
| `power_watts` | float32 | | Power. |
| `coulomb_meter_raw` | float32 | | Coulomb meter, raw. |
| `power_meter_raw` | float32 | | Energy meter, raw. |
| `temperature_celsius` | float32 | | Temperature. |
| `temperature_is_external` | bool | | Whether the temperature is from an external sensor. |
| `power_status_code` | uint8 | | Power status, raw code. |
| `power_status` | string | yes | Power status, as text. |
| `soc_raw` | uint8 | | State of charge, raw. |
| `soc_percent` | uint8 | yes | State of charge, 0–100. Empty if disabled, unknown, or out of range. |
| `runtime_raw` | uint16 | | Runtime, raw. |
| `runtime_minutes` | uint16 | yes | Estimated runtime. Empty if disabled, unknown, or out of range. |
| `rssi_dbm` | int16 | | Signal strength. |

Query examples are in [Query the data](/docs/guides/querying-data/).
