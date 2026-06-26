<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/logo-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="assets/logo.png">
  <img alt="wattdog" src="assets/logo.png" width="320">
</picture>

# wattdog

PowerMon watchdog daemon for PiKVM/Raspberry Pi systems.

wattdog watches Thornwave PowerMon BLE advertisements, records the readings, and calls HTTP endpoints when configured thresholds are crossed. The common use is keeping a Raspberry Pi or PiKVM alive on battery, then safely turning loads off or back on based on measured voltage.

It also exposes `/healthz` and Prometheus `/metrics`, and stores samples as local Parquet files for later inspection.

## Install From Built Assets

GitHub Actions builds release binaries and container images. Prefer these unless you need to change the code or rebuild against a local Thornwave SDK checkout.

### Release Binary

Download the Raspberry Pi 64-bit tarball from the latest GitHub release:

```bash
curl -LO "$(curl -fsSL https://api.github.com/repos/oats-center/wattdog/releases/latest \
  | jq -r '.assets[] | select(.name | endswith("linux-arm64-rpi64.tar.gz")) | .browser_download_url')"
tar -xzf wattdog-*-linux-arm64-rpi64.tar.gz
install -m 0755 wattdog /usr/local/bin/wattdog
```

For x86_64 Linux, use `wattdog-*-linux-amd64.tar.gz` instead.

### Container Image

GitHub Actions also publishes images to GHCR:

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

### Quadlet

Install the Quadlet unit if you want systemd to manage the container:

```bash
install -d /etc/containers/systemd
curl -fsSL https://raw.githubusercontent.com/oats-center/wattdog/main/packaging/wattdog.container \
  -o /etc/containers/systemd/wattdog.container
sed -i 's#Image=localhost/wattdog:latest#Image=ghcr.io/oats-center/wattdog:main#' \
  /etc/containers/systemd/wattdog.container
systemctl daemon-reload
systemctl enable --now wattdog.service
```

Change `Image=` to a release tag, such as `ghcr.io/oats-center/wattdog:v0.1.0`, when you want a pinned version.

## Configuration

By default, the daemon reads:

```text
/etc/wattdog/config.toml
```

Start with one of the example configs:

- `docs/config.example.toml` for the smallest generic example
- `docs/config.rpi-lifepo4.example.toml` for a Raspberry Pi / PiKVM on a 4S LiFePO4 battery

Install it as owner-readable because action URLs may contain tokens or credentials:

```bash
install -o wattdog -g wattdog -m 0750 -d /etc/wattdog
install -o wattdog -g wattdog -m 0600 docs/config.rpi-lifepo4.example.toml /etc/wattdog/config.toml
```

Then edit `/etc/wattdog/config.toml`:

- Set `[data].dir` to the sample storage directory, usually `/var/lib/wattdog`.
- Set `[metrics].listen` to `127.0.0.1:9107` for host-only metrics, or `0.0.0.0:9107` when running in a container with a published port.
- Set each `[[states]].serial` to your PowerMon serial. You can see discovered devices in the `wattdog_device_info` metric.
- Set `field` to the measurement to watch, such as `voltage1_volts`, `voltage2_volts`, `current_amps`, or `power_watts`.
- Set `[states.on]` and `[states.off]` thresholds, durations, and action URLs.

Validate it before running:

```bash
wattdog --config /etc/wattdog/config.toml --check-config
```

Run directly:

```bash
wattdog --config /etc/wattdog/config.toml
```

For a no-network action test:

```bash
wattdog --config /etc/wattdog/config.toml --dry-run
```

## More Details

See `docs/reference.md` for custom builds, container notes, metrics, Parquet layout, DuckDB queries, and retention.

## Credits

This work was supported by the [Open Ag Technologies and Systems (OATS) Center at Purdue University](https://oatscenter.org/), [INDOT](https://www.in.gov/indot/) / [JTRP project SPR-4918](https://engineering.purdue.edu/JTRP/Research#:~:text=SPR-4918), and [IoT4Ag](https://iot4ag.us/).
