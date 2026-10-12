---
title: "Overview"
description: "What wattdog is, what it does, and where to start."
---

wattdog is a watchdog daemon for [Thornwave Labs PowerMon](https://www.thornwave.com/) battery
monitors. It listens for PowerMon Bluetooth Low Energy advertisements, records every reading to
local Parquet files, and calls HTTP endpoints when a measurement crosses a threshold you
configure. It also serves `/healthz` and Prometheus `/metrics`.

It was built for Raspberry Pi and PiKVM systems, but it runs on any Linux host with Bluetooth Low
Energy.

> **Watch the battery. Wait until you're sure. Then act, and keep the record.**

## What wattdog gives you

- **Passive monitoring.** wattdog reads the advertisements PowerMon devices already broadcast.
  There is no pairing and no connection to hold open.
- **Thresholds that wait.** A transition fires only when its condition has held *continuously*
  for its full duration, so a brief dip doesn't shut anything down.
- **Ordered actions.** A transition can call one URL, or run a sequence of requests, delays, and
  JSON polling waits, such as "press the power button, wait for the host to power off, then cut
  the relay".
- **A complete record.** Every advertisement becomes a row in an hourly or daily Parquet file
  you can query with DuckDB, Polars, or Spark.
- **Observability.** `/healthz` and `/metrics` stay up even if the Bluetooth scanner fails to
  start, so you can see what went wrong.
- **Safe to try.** `--check-config` validates a configuration without starting anything, and
  `--dry-run` runs the whole pipeline without sending any HTTP requests.

## How it fits together

```text
BLE scanner → sample processor → state engine → HTTP actions
                    ↓
          Parquet writer + metrics server
```

See [Design](/docs/concepts/design/) for the details and [How thresholds
work](/docs/concepts/thresholds/) for the rules the state engine follows.

## Where to start

| If you want to… | Read |
|---|---|
| Install it | [Install](/docs/getting-started/install/) |
| Protect a Raspberry Pi or PiKVM on a battery | [Quick start](/docs/getting-started/quickstart/) |
| Shut a host down cleanly before cutting power | [PiKVM graceful shutdown](/docs/guides/pikvm-shutdown/) |
| Analyze what happened last night | [Query the data](/docs/guides/querying-data/) |
| Look up a setting | [Configuration reference](/docs/reference/config/) |

## Status

wattdog is pre-1.0. Configuration may change between releases.

wattdog is part of the [OATS Center](https://oatscenter.org/) family of projects, alongside
[Avena](https://oats-center.github.io/avena/), [avena-net](https://oats-center.github.io/avena-net/),
and [Trellis](https://oats-center.github.io/trellis/).
