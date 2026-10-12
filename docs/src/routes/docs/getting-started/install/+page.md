---
title: "Install"
description: "Install wattdog from a release binary, a container image, or a Quadlet unit."
---

wattdog runs on Linux with a Bluetooth Low Energy adapter and BlueZ. Pick one of three ways to
install it.

| Method | Use it when |
|---|---|
| [Release binary](#Release-binary) | You want the simplest install on a Raspberry Pi or an x86_64 host. |
| [Container image](#Container-image) | You already run services in Podman or Docker. |
| [Quadlet](#Quadlet) | You want systemd to manage the container. |

:::caution[Released vs. main]
The latest release is **v0.1.0**. It supports a single `url` per transition. Action sequences
(`actions`) and `hold_for` are on `main` and will ship in the next release. Until then, configs
that use them need a build from `main`. See [Build from source](/docs/reference/building/).
:::

## Release binary

Download the tarball for your architecture from the
[latest release](https://github.com/oats-center/wattdog/releases/latest). Each tarball contains
only the `wattdog` binary.

| Host | Asset |
|---|---|
| Raspberry Pi, 64-bit OS | `wattdog-<version>-linux-arm64-rpi64.tar.gz` |
| x86_64 Linux | `wattdog-<version>-linux-amd64.tar.gz` |

```bash
curl -LO "$(curl -fsSL https://api.github.com/repos/oats-center/wattdog/releases/latest \
  | jq -r '.assets[] | select(.name | endswith("linux-arm64-rpi64.tar.gz")) | .browser_download_url')"
tar -xzf wattdog-*-linux-arm64-rpi64.tar.gz
sudo install -m 0755 wattdog /usr/local/bin/wattdog
wattdog --version
```

The release also has a `SHA256SUMS` file to check the download against.

### Run it as a service

The repository ships a systemd unit, `systemd/wattdog.service`, which runs wattdog as the
`wattdog` user. Nothing creates that user for you, so create it, and the directories, first:

```bash
sudo useradd --system --no-create-home --shell /usr/sbin/nologin wattdog
sudo install -o wattdog -g wattdog -m 0750 -d /etc/wattdog
sudo install -o wattdog -g wattdog -m 0755 -d /var/lib/wattdog

sudo curl -fsSL https://raw.githubusercontent.com/oats-center/wattdog/main/systemd/wattdog.service \
  -o /etc/systemd/system/wattdog.service
sudo systemctl daemon-reload
```

Write `/etc/wattdog/config.toml` (see [Quick start](/docs/getting-started/quickstart/)), then:

```bash
sudo systemctl enable --now wattdog.service
journalctl -u wattdog.service -f
```

The unit uses `ProtectSystem=strict` and only allows writes to `/var/lib/wattdog`. If `[data].dir`
is somewhere else, add it to `ReadWritePaths=`. Depending on the image, the `wattdog` user may also
need extra groups or a D-Bus policy before BlueZ lets it scan.

## Container image

GitHub Actions publishes images for amd64 and arm64 to
`ghcr.io/oats-center/wattdog`. Tags follow the branch (`:main`) and the release (`:v0.1.0`).

```bash
podman pull ghcr.io/oats-center/wattdog:v0.1.0
podman run --rm \
  --name wattdog \
  -p 127.0.0.1:9107:9107 \
  -v /run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro \
  -v /etc/wattdog/config.toml:/etc/wattdog/config.toml:ro \
  -v /var/lib/wattdog:/var/lib/wattdog:Z \
  ghcr.io/oats-center/wattdog:v0.1.0
```

The container uses the host's BlueZ over the system D-Bus socket, so it needs that socket mounted.
Inside a container, set `[metrics].listen = "0.0.0.0:9107"` so the published port reaches it. See
[Run in a container](/docs/guides/containers/) for networking notes.

## Quadlet

The repository's `packaging/wattdog.container` is a Quadlet unit with a read-only root filesystem,
no capabilities, and the same mounts as above.

```bash
sudo install -d /etc/containers/systemd
sudo curl -fsSL https://raw.githubusercontent.com/oats-center/wattdog/main/packaging/wattdog.container \
  -o /etc/containers/systemd/wattdog.container
sudo sed -i 's#Image=localhost/wattdog:latest#Image=ghcr.io/oats-center/wattdog:v0.1.0#' \
  /etc/containers/systemd/wattdog.container
sudo systemctl daemon-reload
sudo systemctl start wattdog.service
```

Quadlet units are enabled through their `[Install]` section when systemd generates them, so
`systemctl enable` isn't needed.

## Next

[Quick start](/docs/getting-started/quickstart/): write a config, check it, and dry-run it.
