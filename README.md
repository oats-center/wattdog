<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo.png">
  <img alt="wattdog" src="assets/logo.png" width="320">
</picture>

# wattdog

[![GitHub Release](https://img.shields.io/github/v/release/oats-center/wattdog?logo=github)](https://github.com/oats-center/wattdog/releases)
[![Container Image](https://img.shields.io/badge/ghcr.io-oats--center%2Fwattdog-blue?logo=podman)](https://github.com/oats-center/wattdog/pkgs/container/wattdog)
[![Rust](https://img.shields.io/badge/rust-2024-orange?logo=rust)](https://www.rust-lang.org/)

A watchdog daemon for [Thornwave Labs PowerMon](https://www.thornwave.com/) devices.

wattdog listens for PowerMon BLE advertisements, records every reading to local Parquet files, and calls HTTP endpoints when configured thresholds are crossed. It also exposes `/healthz` and Prometheus `/metrics` for observability.

Built for Raspberry Pi and PiKVM systems, but runs on any Linux host with Bluetooth Low Energy.

---

## Table of Contents

- [How It Works](#how-it-works)
- [Use Cases](#use-cases)
- [Installation](#installation)
  - [Release Binary](#release-binary)
  - [Container Image](#container-image)
  - [Quadlet](#quadlet)
- [Quick Start](#quick-start)
  - [Configure](#configure)
  - [Validate](#validate)
  - [Test](#test)
  - [Run](#run)
- [Configuration at a Glance](#configuration-at-a-glance)
- [Metrics and Data](#metrics-and-data)
- [Building from Source](#building-from-source)
- [More Details](#more-details)
- [Credits](#credits)

---

## How It Works

```
BLE Scanner → Sample Processor → State Engine → HTTP Actions
                    ↓
            Parquet Writer + Metrics Server
```

1. **BLE Scanner** — Uses the Thornwave SDK to passively scan for PowerMon advertisements over Bluetooth.
2. **Sample Processor** — Normalizes each advertisement into a typed sample and updates metrics.
3. **State Engine** — Evaluates every configured threshold. When a condition holds continuously for its duration, it queues an action.
4. **HTTP Actions** — Calls the configured endpoint with exponential backoff on failure.
5. **Parquet Writer** — Persists every sample to time-partitioned Parquet files in the background.
6. **Metrics Server** — Serves `/healthz` and `/metrics` for Prometheus scraping.

All components run concurrently. If the BLE scanner fails to start, the metrics endpoint stays alive so you can diagnose the problem.

---

## Use Cases

### Battery-Protected Raspberry Pi / PiKVM

The classic use case. A PiKVM or headless Raspberry Pi runs on a 12V LiFePO4 battery with an HTTP-controlled relay or ATX controller. wattdog monitors battery voltage and safely powers the system down before the battery is drained, then powers it back on when the battery has recharged enough.

This is the example configuration in [Quick Start](#quick-start).

### Solar and Off-Grid Load Shedding

Monitor a battery bank and turn off non-critical loads (lighting, secondary radios, auxiliary heaters) when voltage drops. Turn them back on when solar has recovered the bank.

### Remote Site Monitoring

Deploy at unmanned sites. Trigger alerts, failover routines, or notifications when power metrics cross thresholds. The local Parquet history lets you analyze what happened even if the network was down.

### Automated Test Benches

Power cycle equipment under test based on current draw or power consumption. If a device starts drawing too much current, cut power before damage occurs.

---

## Installation

### Release Binary

**Use this when:** you want the simplest deployment on a Raspberry Pi or x86_64 Linux host.

Download the Raspberry Pi 64-bit tarball from the latest GitHub release:

```bash
curl -LO "$(curl -fsSL https://api.github.com/repos/oats-center/wattdog/releases/latest \
  | jq -r '.assets[] | select(.name | endswith("linux-arm64-rpi64.tar.gz")) | .browser_download_url')"
tar -xzf wattdog-*-linux-arm64-rpi64.tar.gz
install -m 0755 wattdog /usr/local/bin/wattdog
```

For x86_64 Linux, use `wattdog-*-linux-amd64.tar.gz` instead.

### Container Image

**Use this when:** you already run PiKVM or other services in Podman/Docker and prefer container management.

GitHub Actions publishes images to GHCR:

```bash
podman pull ghcr.io/oats-center/wattdog:main
podman run --rm \
  --name wattdog \
  -p 127.0.0.1:9107:9107 \
  -v /run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro \
  -v /etc/wattdog/config.toml:/etc/wattdog/config.toml:ro \
  -v /var/lib/wattdog:/var/lib/wattdog:Z \
  ghcr.io/oats-center/wattdog:main
```

Use a release tag, such as `ghcr.io/oats-center/wattdog:v0.1.0`, when you want a pinned version.

The container mounts `/run/dbus/system_bus_socket` because BlueZ exposes Bluetooth scanning over the host system bus.

### Quadlet

**Use this when:** you want systemd to manage the container automatically.

```bash
install -d /etc/containers/systemd
curl -fsSL https://raw.githubusercontent.com/oats-center/wattdog/main/packaging/wattdog.container \
  -o /etc/containers/systemd/wattdog.container
sed -i 's#Image=localhost/wattdog:latest#Image=ghcr.io/oats-center/wattdog:main#' \
  /etc/containers/systemd/wattdog.container
systemctl daemon-reload
systemctl enable --now wattdog.service
```

Change `Image=` to a release tag when you want a pinned version.

---

## Quick Start

### Configure

Create the config directory and drop in the example. This example is tuned for a Raspberry Pi or PiKVM running on a 4S "12V" LiFePO4 battery with an HTTP-controlled relay.

```bash
install -o wattdog -g wattdog -m 0750 -d /etc/wattdog
```

Create `/etc/wattdog/config.toml`:

```toml
# Raspberry Pi / PiKVM 12V LiFePO4 voltage-only example.
#
# Assumes a 4S "12V" LiFePO4 battery and an HTTP-controlled relay or ATX
# controller that accepts POST requests to turn the protected system off/on.
#
# Starting thresholds:
# - OFF at <= 11.2V for 2 minutes: near empty under load, before most BMS cutoffs.
# - ON at >= 13.1V for 10 minutes: roughly recharged enough to avoid rapid cycling.
#
# Tune these from your own Parquet history. LiFePO4 voltage is flat through much
# of the discharge curve, and charger/load wiring can shift these values.

[data]
# Where Parquet files are written. Make sure this directory exists and is writable.
dir = "/var/lib/wattdog"
# Finalize Parquet files every hour. Use "daily" for fewer, larger files.
roll = "hourly"

[metrics]
# Prometheus scrape endpoint. Bind to 127.0.0.1 for local-only access,
# or 0.0.0.0 when running inside a container.
listen = "127.0.0.1:9107"

[http]
# HTTP method for all action URLs.
method = "POST"
# How long to wait for each action endpoint to respond.
timeout = "5s"
# First retry delay after a failed action. Doubles each time, capped at retry_max.
retry_initial = "1s"
retry_max = "5s"
# Security toggles. Enable require_https in production.
require_https = false
allow_invalid_certs = false

[[states]]
name = "rpi_power"
# Replace with your PowerMon serial from the wattdog_device_info metric.
serial = "12345678"
# Use the voltage channel wired to the protected 12V battery bus.
field = "voltage1_volts"
# If the first sample is between thresholds, assume the system should be on.
default_state = "on"
# Pause decisions if no BLE advertisement arrives for 45 seconds.
stale_after = "45s"

[states.on]
# Turn ON when voltage has been >= 13.1V continuously for 10 minutes.
op = ">="
value = 13.1
duration = "10m"
# Replace with your local relay/ATX endpoint for powering the system back on.
url = "http://127.0.0.1:8080/relay/on"

[states.off]
# Turn OFF when voltage has been <= 11.2V continuously for 2 minutes.
op = "<="
value = 11.2
duration = "2m"
# Replace with your local relay/ATX endpoint for safely removing load.
url = "http://127.0.0.1:8080/relay/off"
```

Install it with restricted permissions because action URLs may contain tokens:

```bash
install -o wattdog -g wattdog -m 0600 /etc/wattdog/config.toml /etc/wattdog/config.toml
```

### Validate

Check that the config parses and all thresholds are sane before starting:

```bash
wattdog --config /etc/wattdog/config.toml --check-config
```

### Test

Run in dry-run mode to see what actions would be called without actually hitting the endpoints:

```bash
wattdog --config /etc/wattdog/config.toml --dry-run
```

Watch the logs. You should see BLE advertisements being processed and state decisions being logged. No relays will actually click.

### Run

Start the daemon:

```bash
wattdog --config /etc/wattdog/config.toml
```

When running under systemd:

```bash
journalctl -u wattdog.service -f
```

For scanner startup diagnostics, run with debug logging:

```bash
RUST_LOG=debug wattdog --config /etc/wattdog/config.toml --dry-run
```

---

## Configuration at a Glance

| Section | Purpose |
|---------|---------|
| `[data]` | Where Parquet files are written and how often to roll them. |
| `[metrics]` | Bind address for `/healthz` and Prometheus `/metrics`. |
| `[http]` | Shared HTTP client settings: method, timeout, retry backoff, TLS policy. |
| `[[states]]` | One per logical output. Defines what to watch and which serial to watch. |
| `[states.on]` / `[states.off]` | Threshold, duration, and action URL for each state transition. |

For the full reference, including how the state machine handles continuous duration, stale data, retry backoff, and ambiguous states, see [`docs/config.md`](docs/config.md).

---

## Metrics and Data

### Health and Metrics Endpoints

```bash
curl http://127.0.0.1:9107/healthz
curl http://127.0.0.1:9107/metrics
```

Important metrics include:

- `wattdog_up` — Daemon health
- `wattdog_ble_scanner_running` — Whether BLE scanning is active
- `wattdog_observations_received_total` — Total BLE advertisements processed
- `wattdog_voltage1_volts{serial="..."}` — Latest voltage reading per device
- `wattdog_state_desired{name="..."}` — What the state engine wants to do
- `wattdog_state_applied{name="..."}` — What has been successfully applied
- `wattdog_http_attempts_total{name="...",target="on\|off",result="success\|failure"}` — Action success/failure counts

See [`docs/reference.md`](docs/reference.md) for the complete metrics list, Parquet layout, DuckDB query examples, and retention guidance.

### Parquet Files

Samples are written to time-partitioned Parquet files:

```text
/var/lib/wattdog/samples/date=YYYY-MM-DD/hour=HH/part-YYYYMMDDTHH0000Z.parquet
```

Active files use `.parquet.inprogress` and are renamed only after the writer closes successfully. Downstream readers should ignore `.inprogress` files.

---

## Building from Source

The Thornwave SDK is a mandatory build-time dependency. Clone it into the ignored `vendor/libpowermon_bin` path:

```bash
mkdir -p vendor
git clone https://git.thornwave.com/git/thornwave/libpowermon_bin.git vendor/libpowermon_bin
cargo build --release
```

For cross-compilation to aarch64/Raspberry Pi 64-bit:

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-gnu
```

See [`docs/reference.md`](docs/reference.md) for full build details, including custom SDK paths and local container builds.

---

## More Details

- [`docs/config.md`](docs/config.md) — Full configuration reference and state machine behavior
- [`docs/reference.md`](docs/reference.md) — Build notes, container details, metrics reference, Parquet layout, DuckDB queries, retention

---

## Credits

This work was supported by the [Open Ag Technologies and Systems (OATS) Center at Purdue University](https://oatscenter.org/), [INDOT](https://www.in.gov/indot/) / [JTRP project SPR-4918](https://engineering.purdue.edu/JTRP/Research#:~:text=SPR-4918), and [IoT4Ag](https://iot4ag.us/).
