---
title: "Quick start"
description: "Protect a Raspberry Pi or PiKVM on a 12 V LiFePO4 battery: configure, validate, dry-run, run."
---

This page sets up the classic case. A Raspberry Pi or PiKVM runs from a 4S "12 V" LiFePO4
battery through a relay you can switch over HTTP. wattdog cuts the load before the battery is
drained and restores it once the battery has recovered.

You need wattdog [installed](/docs/getting-started/install/), a PowerMon wired to the battery, and
the relay's on and off URLs.

## 1. Find your PowerMon's serial

The serial is printed on the PowerMon's label. It is a **decimal** integer, not hex.

If you can't read the label, start wattdog with any valid config. Then look for
`wattdog_device_info` in the metrics. It lists every PowerMon the scanner has heard, with its
`serial`, `name`, `model`, and Bluetooth `address`.

```bash
curl -s http://127.0.0.1:9107/metrics | grep wattdog_device_info
```

## 2. Write the config

Create `/etc/wattdog/config.toml`:

```toml
# 4S "12V" LiFePO4 battery with an HTTP-controlled relay.
#
# OFF at <= 11.2 V for 2 minutes: near empty under load, before most BMS cutoffs.
# ON  at >= 13.1 V for 10 minutes: recharged enough to avoid rapid cycling.
# Tune these from your own Parquet history. LiFePO4 voltage is flat through much
# of the discharge curve, and wiring and chargers shift these values.

[data]
dir = "/var/lib/wattdog"
roll = "hourly"

[metrics]
listen = "127.0.0.1:9107"

[http]
method = "POST"
timeout = "5s"
retry_initial = "1s"
retry_max = "5s"

[[states]]
name = "rpi_power"
serial = "12345678"            # your PowerMon's decimal serial
field = "voltage1_volts"       # the channel wired to the battery bus
default_state = "on"
stale_after = "45s"

[states.on]
op = ">="
value = 13.1
duration = "10m"
url = "http://127.0.0.1:8080/relay/on"

[states.off]
op = "<="
value = 11.2
duration = "2m"
url = "http://127.0.0.1:8080/relay/off"
```

Action URLs can carry credentials, so keep the file private:

```bash
sudo chown wattdog:wattdog /etc/wattdog/config.toml
sudo chmod 0600 /etc/wattdog/config.toml
```

## 3. Validate it

`--check-config` parses and validates the file, then exits without starting the scanner.

```console
$ wattdog --config /etc/wattdog/config.toml --check-config
INFO wattdog: loaded wattdog config config=/etc/wattdog/config.toml
INFO wattdog: config is valid config=/etc/wattdog/config.toml
```

Validation catches mistakes that would otherwise show up at 3 a.m. For example, if the off
threshold were above the on threshold:

```console
$ wattdog --config bad.toml --check-config
Error: state rpi_power on/off thresholds overlap
```

## 4. Dry-run it

`--dry-run` runs everything — scanning, recording, threshold timers, and metrics — but treats
every HTTP step as successful without sending a request. No relays click.

```bash
wattdog --config /etc/wattdog/config.toml --dry-run
```

```console
INFO wattdog: starting wattdog dry_run=true
INFO wattdog::http_server: metrics HTTP server listening addr=127.0.0.1:9107
INFO wattdog: started Thornwave BLE scan
INFO wattdog: scanner is running; metrics will remain empty until Thornwave advertisements are received
```

Within a minute you should see `processed Thornwave advertisements` in the log, and
`wattdog_voltage1_volts{serial="…"}` in `/metrics`. If the log keeps saying `no Thornwave
advertisements have been processed yet`, check that the PowerMon is in range and the serial is
right. Run with `RUST_LOG=debug` for scanner details.

When a threshold fires during a dry run, the log shows a `dry-run HTTP action` line with the
state, the target, and the host it would have called.

## 5. Run it

```bash
sudo systemctl enable --now wattdog.service
journalctl -u wattdog.service -f
```

:::note[What happens at startup]
wattdog doesn't know the relay's state when it starts, so it drives the output to a known state.
If the first reading is between the thresholds, it calls `default_state`'s action (here, **on**).
If the reading already satisfies a threshold, wattdog waits for that threshold's duration and then
acts. Either way, expect one action call shortly after every start. That's why actions must be
idempotent.
:::

## Next

- [How thresholds work](/docs/concepts/thresholds/): continuous duration, stale data, and dwell.
- [PiKVM graceful shutdown](/docs/guides/pikvm-shutdown/): shut the host down before cutting power.
- [Query the data](/docs/guides/querying-data/): check your thresholds against real history.
