# wattdog

PowerMon watchdog daemon for PiKVM/Raspberry Pi systems.

The daemon scans Thornwave BLE advertisements through Thornwave's native SDK, serves `/healthz` and Prometheus-compatible `/metrics`, stores observed samples as local Parquet files, and uses configured thresholds to drive HTTP actions. In `--dry-run` mode it records action success without sending HTTP requests.

## Build requirements

The Thornwave SDK is a mandatory build-time dependency, but it is not committed to this repository. Install or unpack it at `vendor/libpowermon_bin`, or point `THORNWAVE_SDK_DIR` at another local SDK path. By default, `build.rs` uses `vendor/libpowermon_bin` and selects the static library by target architecture:

- `powermon_lib_pic.a` for normal x86_64 Linux development builds
- `powermon_lib_rpi64_pic.a` for aarch64/Raspberry Pi 64-bit builds

Build with:

```bash
cargo build --release
```

To override the SDK path or library filename:

```bash
export THORNWAVE_SDK_DIR=/opt/libpowermon_bin
export THORNWAVE_LIB_FILE=powermon_lib_rpi64_pic.a
cargo build --release
```

`build.rs` requires:

- `$THORNWAVE_SDK_DIR/inc/powermon.h`
- `$THORNWAVE_SDK_DIR/inc/powermon_scanner.h`
- `$THORNWAVE_SDK_DIR/$THORNWAVE_LIB_FILE`

The build links the Thornwave static library plus `stdc++`, `bluetooth`, and `dbus-1`.

## Cross-compile for Raspberry Pi 64-bit

The recommended cross-build path is [`cross`](https://github.com/cross-rs/cross). This crate includes `Cross.toml` and a Fedora 42 based `Dockerfile.aarch64-unknown-linux-gnu` for `aarch64-unknown-linux-gnu`.

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-gnu
```

If you previously tried a manual host cross-build and see build-script `GLIBC_* not found` errors, clear the stale target artifacts once:

```bash
cargo clean --target aarch64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
```

The resulting binary is:

```text
target/aarch64-unknown-linux-gnu/release/wattdog
```

For manual cross-compiles, install the Rust target and an aarch64 Linux cross toolchain, then build with an aarch64 linker:

```bash
rustup target add aarch64-unknown-linux-gnu
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
  cargo build --release --target aarch64-unknown-linux-gnu
```

The cross linker must be able to find target `stdc++`, BlueZ, and DBus libraries.

## Configuration

By default the daemon reads:

```text
/etc/wattdog/config.toml
```

Start from `docs/config.example.toml`, or `docs/config.rpi-lifepo4.example.toml` for a Raspberry Pi 4S LiFePO4 voltage-only shutdown/restart example. Validate it before running:

```bash
wattdog --config /etc/wattdog/config.toml --check-config
```

Config files may contain action URLs with userinfo, tokens, or other secrets. Install production config as owner-readable only:

```bash
install -o wattdog -g wattdog -m 0750 -d /etc/wattdog
install -o wattdog -g wattdog -m 0600 docs/config.example.toml /etc/wattdog/config.toml
```

## Run

```bash
wattdog --config /etc/wattdog/config.toml
```

For a no-network action test:

```bash
wattdog --config /etc/wattdog/config.toml --dry-run
```

The daemon logs at `info` by default. For scanner startup diagnostics:

```bash
RUST_LOG=debug ./wattdog --config ./docs/config.example.toml --dry-run
```

When running under systemd:

```bash
journalctl -u wattdog.service -f
```

## Run With Podman

Build the Raspberry Pi binary first, then build the Fedora 42 runtime image from that binary:

```bash
cross build --release --target aarch64-unknown-linux-gnu
podman build --arch arm64 -f packaging/Containerfile -t localhost/wattdog:latest .
```

## GitHub Actions Builds

`.github/workflows/build.yml` builds `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`, uploads both binaries, publishes GHCR images tagged with `-amd64`, `-arm64`, and `-rpi64` suffixes, and creates unsuffixed multi-arch image tags.

The workflow fetches the public Thornwave SDK from `https://git.thornwave.com/git/thornwave/libpowermon_bin.git`, which must contain:

```text
inc/powermon.h
inc/powermon_scanner.h
powermon_lib_pic.a
powermon_lib_rpi64_pic.a
```

32-bit Raspberry Pi builds are intentionally not enabled because the bundled Thornwave SDK archives are x86_64 and AArch64 only.

For container use, set metrics to listen inside the container and publish it only on host localhost:

```toml
[metrics]
listen = "0.0.0.0:9107"
```

Run directly with only the host resources this daemon should need:

```bash
podman run --rm \
  --name wattdog \
  --network bridge \
  -p 127.0.0.1:9107:9107 \
  --read-only \
  --tmpfs /tmp \
  --security-opt no-new-privileges \
  --cap-drop all \
  -v /run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro \
  -v /etc/wattdog/config.toml:/etc/wattdog/config.toml:ro \
  -v /var/lib/wattdog:/var/lib/wattdog:Z \
  localhost/wattdog:latest
```

Install the Quadlet unit if you want systemd to manage the container:

```bash
install -m 0644 packaging/wattdog.container /etc/containers/systemd/wattdog.container
systemctl daemon-reload
systemctl enable --now wattdog.service
```

The container mounts `/run/dbus/system_bus_socket` because BlueZ exposes Bluetooth scanning over the host system bus. Keep SELinux labeling enabled by default; if BlueZ access fails with SELinux AVC denials, prefer a narrow host policy fix before trying `--security-opt label=disable` as a diagnostic shortcut.

Bridge networking is the default because the daemon only needs outbound HTTP actions and one published metrics port. Use LAN addresses, another container/pod address, or Podman's host gateway name in action URLs. For an action service running on the same Raspberry Pi host, use `http://host.containers.internal:8080/...` instead of `http://127.0.0.1:8080/...`. Switch to `--network host` only if the action service is bound to host loopback and cannot be changed.

## Metrics

```bash
curl http://127.0.0.1:9107/healthz
curl http://127.0.0.1:9107/metrics
```

Important sample/storage metrics include:

- `wattdog_up`
- `wattdog_ble_scanner_running`
- `wattdog_observations_received_total`
- `wattdog_observations_dropped_total`
- `wattdog_writer_dropped_observations_total`
- `wattdog_devices_seen`
- `wattdog_voltage1_volts{serial="..."}`
- `wattdog_voltage2_volts{serial="..."}`
- `wattdog_current_amperes{serial="..."}`
- `wattdog_power_watts{serial="..."}`
- `wattdog_parquet_rows_written_total`
- `wattdog_parquet_write_errors_total`
- `wattdog_state_desired{name="..."}`
- `wattdog_state_applied{name="..."}`
- `wattdog_http_attempts_total{name="...",target="on|off",result="success|failure"}`

Action URLs are never used as metric labels.

## Parquet layout

Hourly mode:

```text
/var/lib/wattdog/samples/date=YYYY-MM-DD/hour=HH/part-YYYYMMDDTHH0000Z.parquet
```

Daily mode:

```text
/var/lib/wattdog/samples/date=YYYY-MM-DD/part-YYYYMMDD.parquet
```

Active files use `.parquet.inprogress` and are renamed only after the Parquet writer closes successfully. Downstream readers should ignore `.inprogress` files.

## Alloy scrape example

```hcl
prometheus.scrape "wattdog" {
  targets = [
    { __address__ = "127.0.0.1:9107", job = "wattdog" },
  ]

  scrape_interval = "15s"
  forward_to      = [prometheus.remote_write.cloud.receiver]
}
```

## Querying with DuckDB

```sql
SELECT
  date_trunc('minute', observed_at_utc) AS minute,
  serial,
  avg(voltage1_volts) AS avg_v1,
  avg(current_amps) AS avg_current,
  avg(power_watts) AS avg_power
FROM read_parquet('/var/lib/wattdog/samples/**/*.parquet')
GROUP BY minute, serial
ORDER BY minute, serial;
```

## Retention

Retention is external. Do not use logrotate to rotate or truncate active Parquet files. Cleanup jobs should delete only completed `*.parquet` files.
