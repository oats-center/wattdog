# wattdog Configuration

wattdog is configured through a single TOML file. By default it reads `/etc/wattdog/config.toml`.

Because action URLs may contain tokens or credentials, treat the config file as a secret:

```bash
install -o wattdog -g wattdog -m 0750 -d /etc/wattdog
install -o wattdog -g wattdog -m 0600 docs/config.rpi-lifepo4.example.toml /etc/wattdog/config.toml
```

Validate before running:

```bash
wattdog --config /etc/wattdog/config.toml --check-config
```

---

## Table of Contents

- [Top-level Sections](#top-level-sections)
  - [`[data]` — Sample Storage](#data--sample-storage)
  - [`[metrics]` — Prometheus Endpoint](#metrics--prometheus-endpoint)
  - [`[http]` — Action Client](#http--action-client)
  - [`[[states]]` — Threshold Outputs](#states--threshold-outputs)
    - [`[states.on]` / `[states.off]` — Transitions](#stateson--statesoff--transitions)
- [How Thresholds Work](#how-thresholds-work)
  - [Continuous Duration](#continuous-duration)
  - [Opposite Directions](#opposite-directions)
  - [Stale Data](#stale-data)
  - [Ambiguous States](#ambiguous-states)
  - [Retry Backoff](#retry-backoff)
  - [Default State](#default-state)
- [Finding Your Serial](#finding-your-serial)
- [Minimal Example](#minimal-example)

---

## Top-level Sections

### `[data]` — Sample Storage

Where wattdog writes Parquet files containing every observed BLE advertisement.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `dir` | string | *(required)* | Directory for Parquet output. Usually `/var/lib/wattdog`. |
| `roll` | string | `"hourly"` | How often to finalize the active Parquet file. `"hourly"` or `"daily"`. |

`roll` affects partition granularity. Hourly rolls create more files but make time-range queries faster. Daily rolls create fewer files, better for long-term archival.

### `[metrics]` — Prometheus Endpoint

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `listen` | string | `"127.0.0.1:9107"` | HTTP bind address for `/healthz` and `/metrics`. |

Use `127.0.0.1:9107` for host-only access. Use `0.0.0.0:9107` when running inside a container with published ports.

### `[http]` — Action Client

Shared HTTP settings for all action URLs across all states.

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `method` | string | *(required)* | `"POST"` or `"PUT"`. |
| `timeout` | duration | `"5s"` | Per-request timeout. |
| `retry_initial` | duration | `"1s"` | First retry delay after a failed action. |
| `retry_max` | duration | `"5s"` | Maximum retry delay. Capped at 5 seconds. |
| `require_https` | bool | `false` | Reject action URLs that are not HTTPS. |
| `allow_invalid_certs` | bool | `false` | Allow invalid TLS certificates. Use only in lab environments. |

Retry delays grow exponentially: `1s`, `2s`, `4s`, up to `retry_max`. A successful action resets the backoff.

### `[[states]]` — Threshold Outputs

Each `[[states]]` block defines one binary output controlled by one PowerMon measurement. You can have multiple states watching the same serial with different fields, or different serials entirely.

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Unique identifier. Used in metrics and logs. |
| `serial` | string | Decimal PowerMon serial number. Not hex. |
| `field` | string | Measurement to watch: `voltage1_volts`, `voltage2_volts`, `current_amps`, `power_watts`. |
| `default_state` | string | `"on"` or `"off"`. Assumed when the first sample is between thresholds. |
| `stale_after` | duration | Pause decisions if no advertisement is seen for this long. |

#### `[states.on]` / `[states.off]` — Transitions

Each state has exactly two transitions. When a transition's condition is met continuously for its duration, wattdog calls the transition's URL to move the output to that state.

| Field | Type | Description |
|-------|------|-------------|
| `op` | string | Comparison operator: `<`, `<=`, `>`, `>=`. |
| `value` | float | Threshold value. |
| `duration` | duration | How long the condition must hold without interruption. |
| `url` | string | HTTP endpoint to call when this transition fires. |

---

## How Thresholds Work

wattdog's state machine is simple but precise. Understanding these rules prevents surprises.

### Continuous Duration

A transition only fires when its condition holds **continuously** for the full `duration`. One sample that breaks the condition resets the timer to zero.

Example: `duration = "2m"` with `op = "<="` and `value = 11.2`. The voltage must stay at or below 11.2V for two full minutes. A single reading of 11.3V resets the clock.

This prevents flickering: a brief dip doesn't kill your system.

### Opposite Directions

The `on` and `off` transitions must point in opposite directions. If `on` uses `<=`, `off` must use `>` or `>=`. If `on` uses `>=`, `off` must use `<` or `<=`.

wattdog validates this at startup and refuses to start with overlapping or same-direction thresholds.

### Stale Data

If no BLE advertisement from a state's serial arrives within `stale_after`, the state machine pauses new decisions. The `desired_state` becomes `None`. No new actions are attempted.

However, if an action was already pending (a previous attempt failed and is waiting to retry), that retry still fires even while stale. wattdog doesn't abandon in-flight work just because the radio went quiet.

### Ambiguous States

If both `on` and `off` conditions are somehow satisfied simultaneously (usually a configuration error), wattdog does nothing. It logs the ambiguity and waits for the conditions to diverge. This is a safety guard, not a bug.

### Retry Backoff

When an action URL fails, wattdog retries with exponential backoff starting at `retry_initial` and doubling each time, capped at `retry_max`.

| Attempt | Delay (initial=1s, max=5s) |
|---------|---------------------------|
| 1 | 1s |
| 2 | 2s |
| 3 | 4s |
| 4+ | 5s |

A successful action clears the backoff and resets the retry counter.

### Default State

On first boot, if the first sample's value is between both thresholds (neither condition is met), wattdog assumes `default_state` as the desired state. This prevents a "neither on nor off" limbo.

Once a threshold is crossed and an action successfully applied, `default_state` is no longer used. The system runs on measured state from then on.

---

## Finding Your Serial

PowerMon serial numbers are printed on the device label. They are decimal integers, not hex.

If you don't know the serial, start wattdog with a placeholder config and check the metrics:

```bash
wattdog --config /etc/wattdog/config.toml
# In another terminal:
curl -s http://127.0.0.1:9107/metrics | grep wattdog_device_info
```

The `wattdog_device_info` metric lists every PowerMon the scanner has seen, including serial, model, firmware version, and signal strength.

---

## Minimal Example

This is the smallest valid config. It watches one PowerMon and turns a relay on when voltage drops to 12V or below for 30 seconds, and off when it rises to 12.6V or above for 5 minutes.

```toml
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
name = "battery_relay"
serial = "12345678"
field = "voltage1_volts"
default_state = "off"
stale_after = "30s"

[states.on]
op = "<="
value = 12.0
duration = "30s"
url = "http://127.0.0.1:8080/relay/on"

[states.off]
op = ">="
value = 12.6
duration = "5m"
url = "http://127.0.0.1:8080/relay/off"
```
