---
title: "Run in a container"
description: "Bluetooth, networking, and file permissions when wattdog runs under Podman or Docker."
---

The published image (`ghcr.io/oats-center/wattdog`) is a Fedora base with the `wattdog` binary as
its entrypoint. Its default command is `--config /etc/wattdog/config.toml`. See
[Install](/docs/getting-started/install/#Container-image) for the `podman run` and Quadlet
commands.

## Bluetooth

wattdog talks to the host's BlueZ daemon over D-Bus. The container needs the host's system bus
socket, and nothing else:

```bash
-v /run/dbus/system_bus_socket:/run/dbus/system_bus_socket:ro
```

It doesn't need `--privileged`, `--net=host`, or any capabilities. The shipped Quadlet unit drops
them all.

## The metrics port

Inside a container, `127.0.0.1` is the container's own loopback. Set:

```toml
[metrics]
listen = "0.0.0.0:9107"
```

Then publish the port only to the host: `-p 127.0.0.1:9107:9107`.

## Reaching your actuators

Action URLs are resolved from inside the container.

- For a device on the LAN (a relay board, a PiKVM), use its LAN address or name, as usual.
- For a service on the same host, use `http://host.containers.internal/...` (Podman) or
  `host.docker.internal` (Docker).
- Switch to `--network host` only if the service listens on the host's loopback and you can't
  change that.

## Files

| Host path | Container path | Mode |
|---|---|---|
| `/etc/wattdog/config.toml` | `/etc/wattdog/config.toml` | read-only |
| `/var/lib/wattdog` | `/var/lib/wattdog` | read-write, `:Z` relabels it for SELinux |

The config file holds action credentials. Keep it at mode `0600` on the host.
