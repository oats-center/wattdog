---
title: "Configuration"
description: "Every section and field of wattdog's TOML config, with defaults and validation rules."
---

wattdog reads one TOML file, `/etc/wattdog/config.toml` by default (change it with `--config`).
Unknown keys are errors. Durations are strings like `"500ms"`, `"45s"`, `"2m"`, or `"1h"`.

Action URLs can contain credentials, so keep the file at mode `0600`. Check a file without starting
anything with:

```bash
wattdog --config /etc/wattdog/config.toml --check-config
```

## `[data]`

Where samples are stored.

| Field | Type | Default | Description |
|---|---|---|---|
| `dir` | path | *required* | Directory for Parquet output, usually `/var/lib/wattdog`. Files go in its `samples/` subdirectory. |
| `roll` | string | `"hourly"` | When to close the active file and start a new one: `"hourly"` or `"daily"` (UTC). |

Hourly files make time-range queries faster. Daily files are fewer and better for long-term
archives.

## `[metrics]`

Optional.

| Field | Type | Default | Description |
|---|---|---|---|
| `listen` | socket address | `"127.0.0.1:9107"` | Bind address for `/healthz` and `/metrics`. Use `0.0.0.0:9107` inside a container. |

## `[http]`

Shared by every action in every state.

| Field | Type | Default | Description |
|---|---|---|---|
| `method` | string | *required* | Default request method: `"GET"`, `"POST"`, or `"PUT"`. |
| `timeout` | duration | `"5s"` | Per-request timeout. |
| `retry_initial` | duration | `"1s"` | First delay after a failed request. Must be > 0. |
| `retry_max` | duration | `"5s"` | Largest retry delay. Must be ≥ `retry_initial` and ≤ 5 s. |
| `require_https` | bool | `false` | Reject action URLs that aren't HTTPS. |
| `allow_invalid_certs` | bool | `false` | Accept invalid TLS certificates. Use only in a lab. |

## `[[states]]`

One block per on/off output. At least one is required. Several states can watch the same
PowerMon (different fields) or different ones.

| Field | Type | Description |
|---|---|---|
| `name` | string | Unique, non-empty name. Used in logs and metric labels. |
| `serial` | string | PowerMon serial, as a **decimal** integer in a string (`"12345678"`). |
| `field` | string | What to watch: `voltage1_volts`, `voltage2_volts`, `current_amps`, or `power_watts`. |
| `default_state` | string | `"on"` or `"off"`. The target at startup when the first reading is in the neutral band. |
| `stale_after` | duration | Pause new decisions if no advertisement arrives for this long. Must be > 0. |

### `[states.on]` and `[states.off]`

Both are required. Each describes when to move the output to that state, and how.

| Field | Type | Description |
|---|---|---|
| `op` | string | `"<"`, `"<="`, `">"`, or `">="`. |
| `value` | float | Threshold. |
| `duration` | duration | How long the condition must hold without interruption. `"0s"` acts on the first matching reading. |
| `hold_for` | duration | Minimum time to stay in this state after its action completes. Default `"0s"`. *(main)* |
| `url` | URL | One request using `[http].method`. |
| `actions` | array | An ordered sequence of steps (below). *(main)* |

Set **exactly one** of `url` or `actions`.

The two transitions must point in opposite directions (one `<`/`<=`, the other `>`/`>=`), and
their ranges must not overlap. If `off` is `<= 11.2`, `on` can be `>= 11.3`, `> 11.2`, or anything
higher, but not `>= 11.2`.

:::note[main]
Fields marked *(main)* aren't in v0.1.0. They'll ship in the next release.
:::

### Action steps

Each entry in `actions` is a table with a `type`.

**`request`**: send one request and move on after a successful response.

| Field | Required | Description |
|---|---|---|
| `url` | yes | `http` or `https` URL. A `user:password@` part is sent as basic auth. |
| `method` | no | Overrides `[http].method` for this step. |

**`delay`**: wait, without holding up other states.

| Field | Required | Description |
|---|---|---|
| `duration` | yes | At least `"1s"`. |

**`wait_for_json`**: poll with GET until a value in the JSON response matches.

| Field | Required | Description |
|---|---|---|
| `url` | yes | `http` or `https` URL to poll. |
| `pointer` | yes | [RFC 6901](https://www.rfc-editor.org/rfc/rfc6901) JSON Pointer, such as `"/result/leds/power"`. |
| `expected` | yes | The exact JSON value required at `pointer` (`false`, `"off"`, `0`, …). |
| `poll_every` | yes | Time between polls. At least `"1s"`, and no more than `timeout`. |
| `timeout` | yes | How long to wait. At least `"1s"`. |
| `on_timeout` | yes | `"continue"`: record the timeout and go to the next step. `"retry"`: start another wait after the retry backoff. |

```toml
[[states.off.actions]]
type = "request"
url = "https://admin:password@pikvm/api/atx/power?action=off&wait=1"

[[states.off.actions]]
type = "wait_for_json"
url = "https://admin:password@pikvm/api/atx"
pointer = "/result/leds/power"
expected = false
poll_every = "5s"
timeout = "2m"
on_timeout = "continue"

[[states.off.actions]]
type = "delay"
duration = "5s"
```

## Minimal config

```toml
[data]
dir = "/var/lib/wattdog"

[http]
method = "POST"

[[states]]
name = "battery_relay"
serial = "12345678"
field = "voltage1_volts"
default_state = "on"
stale_after = "30s"

[states.on]
op = ">="
value = 12.6
duration = "5m"
url = "http://127.0.0.1:8080/relay/on"

[states.off]
op = "<="
value = 12.0
duration = "30s"
url = "http://127.0.0.1:8080/relay/off"
```

This turns the relay off after 30 seconds at or below 12.0 V. It turns it back on after 5 minutes at
or above 12.6 V.

## Validation errors

| Message | Fix |
|---|---|
| `at least one state is required` | Add a `[[states]]` block. |
| `duplicate state name …` | Give every state a unique `name`. |
| `state … serial must be an unsigned decimal integer` | Use the decimal serial, not hex. |
| `state … on/off thresholds must use opposite directions` | Make one transition `<`/`<=` and the other `>`/`>=`. |
| `state … on/off thresholds overlap` | Move the thresholds apart so there's a neutral band. |
| `state … must configure exactly one of url or actions` | Remove one, or add one. |
| `state … URL must use https` | `require_https` is on. Use `https://`. |
| `http.retry_max must be <= 5s` | Lower `retry_max`. |
