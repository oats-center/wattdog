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

## Features

- **Passive Bluetooth scanning.** wattdog reads the advertisements PowerMon devices broadcast. It
  doesn't pair or hold a connection.
- **Duration-qualified thresholds.** A transition fires only when its condition has held
  continuously for its configured duration.
- **Action sequences.** A transition calls one URL, or runs a sequence of requests, delays, and
  JSON polling waits.
- **Parquet history.** Every advertisement is written as a row to an hourly or daily Parquet
  file.
- **Health and metrics.** `/healthz` and Prometheus `/metrics`. They stay available if the
  Bluetooth scanner fails to start.
- **Validation and dry runs.** `--check-config` validates a configuration and exits. `--dry-run`
  runs everything without sending HTTP requests.

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
