---
title: "Command line"
description: "wattdog's flags, environment variables, signals, and exit codes."
---

```console
$ wattdog --help
PowerMon threshold watchdog daemon

Usage: wattdog [OPTIONS]

Options:
      --config <CONFIG>  TOML configuration file path [default: /etc/wattdog/config.toml]
      --check-config     Validate configuration and exit without starting scanner/tasks
      --dry-run          Parse dry-run mode for the later action phase
  -h, --help             Print help
  -V, --version          Print version
```

| Flag | Effect |
|---|---|
| `--config <path>` | Config file to load. Default `/etc/wattdog/config.toml`. |
| `--check-config` | Load and validate the config, then exit. It doesn't start the scanner, writer, or metrics server. |
| `--dry-run` | Run everything, but treat every HTTP request and poll as an immediate success without sending it. Each skipped call is logged as `dry-run HTTP action`. |
| `-V`, `--version` | Print the version. |

## Environment

| Variable | Effect |
|---|---|
| `RUST_LOG` | Log filter. The default is `info`. `RUST_LOG=debug` adds scanner diagnostics. |

## Signals

SIGINT (Ctrl-C) and SIGTERM both shut wattdog down cleanly. It stops the scanner, finishes the
active Parquet file, renames it to `.parquet`, and exits. A transition in progress is abandoned. On
the next start, wattdog drives the output to a known state again (see
[startup](/docs/concepts/thresholds/#Startup-and-default_state)).

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Clean shutdown, or `--check-config` found the config valid. |
| `1` | The config couldn't be read, parsed, or validated, or startup failed. The reason is printed as `Error: …`. |
