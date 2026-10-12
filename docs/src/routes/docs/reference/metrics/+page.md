---
title: "Metrics"
description: "Every metric wattdog exposes at /metrics, plus /healthz and a scrape example."
---

wattdog serves two endpoints on `[metrics].listen` (default `127.0.0.1:9107`):

| Path | Returns |
|---|---|
| `/healthz` | `ok` while the daemon is running. |
| `/metrics` | OpenMetrics text (`application/openmetrics-text; version=1.0.0`). |

Counters carry a `_total` suffix. State values use `on = 1`, `off = 0`, and `unknown = -1`.

## Daemon

| Metric | Type | Meaning |
|---|---|---|
| `wattdog_build_info{version}` | gauge | Always `1`. Carries the version. |
| `wattdog_up` | gauge | `1` while any scanner transport is running. |
| `wattdog_ble_scanner_running` | gauge | `1` while the BLE scanner is running. |
| `wattdog_observations_received_total` | counter | Advertisements received. |
| `wattdog_observations_dropped_total` | counter | Advertisements dropped because the sample queue was full. |
| `wattdog_writer_dropped_observations_total` | counter | Samples dropped because the Parquet writer queue was full. |
| `wattdog_devices_seen` | gauge | Distinct PowerMon serials heard. |

## Devices

Each is labeled `serial` and shows the latest advertised value.

| Metric | Type | Meaning |
|---|---|---|
| `wattdog_device_info{serial,name,model,address}` | gauge | Always `1`. Identifies each PowerMon heard. |
| `wattdog_device_observations_received_total` | counter | Advertisements from this device. |
| `wattdog_last_observation_time_seconds` | gauge | Unix time of its last advertisement. |
| `wattdog_voltage1_volts`, `wattdog_voltage2_volts` | gauge | Voltage channels 1 and 2. |
| `wattdog_current_amperes` | gauge | Current. |
| `wattdog_power_watts` | gauge | Power. |
| `wattdog_temperature_celsius` | gauge | Temperature. |
| `wattdog_soc_percent` | gauge | State of charge. |
| `wattdog_runtime_minutes` | gauge | Estimated runtime. |
| `wattdog_rssi_dbm` | gauge | Signal strength. |
| `wattdog_power_status_code` | gauge | PowerMon power status code. |

## Parquet writer

| Metric | Type | Meaning |
|---|---|---|
| `wattdog_parquet_rows_written_total` | counter | Rows written. |
| `wattdog_parquet_batches_written_total` | counter | Record batches written. |
| `wattdog_parquet_write_errors_total` | counter | Write errors. |
| `wattdog_parquet_active_file_open` | gauge | `1` while an `.inprogress` file is open. |
| `wattdog_parquet_active_file_rows` | gauge | Rows in the active file. |
| `wattdog_parquet_last_successful_write_timestamp_seconds` | gauge | Unix time of the last successful write. |
| `wattdog_parquet_rolls_total` | counter | Files completed and renamed. |

## States

Each is labeled with the state's `name`.

| Metric | Type | Meaning |
|---|---|---|
| `wattdog_state_input_value{name,field}` | gauge | Latest value of the watched field. |
| `wattdog_state_input_age_seconds` | gauge | Age of that value. |
| `wattdog_state_stale` | gauge | `1` when the input is older than `stale_after`. |
| `wattdog_state_ambiguous` | gauge | `1` if both transitions are satisfied. A valid config can't reach this state. |
| `wattdog_state_desired` | gauge | What the engine wants: `1`, `0`, or `-1`. |
| `wattdog_state_applied` | gauge | What has been successfully applied: `1`, `0`, or `-1`. |
| `wattdog_state_pending` | gauge | `1` while a transition is in progress. |
| `wattdog_state_default` | gauge | The configured `default_state`. |
| `wattdog_state_transitions_total{name,target}` | counter | Desired-state transitions. |
| `wattdog_action_sequence_step` | gauge | Zero-based step in progress, or `-1` when idle. |
| `wattdog_action_wait_timeouts_total{name,target}` | counter | `wait_for_json` steps that reached their timeout. |
| `wattdog_http_attempts_total{name,target,result}` | counter | Requests, with `result` either `success` or `failure`. |
| `wattdog_http_last_status_code` | gauge | Last HTTP status. |
| `wattdog_http_retry_delay_seconds` | gauge | Current retry delay. |
| `wattdog_http_pending` | gauge | Same as `wattdog_state_pending`. |

Action URLs are never used as labels.

## Alerts worth having

- `wattdog_ble_scanner_running == 0`: the scanner isn't running.
- `wattdog_state_stale == 1` for longer than a few minutes: the PowerMon is out of range or off.
- `wattdog_state_desired != wattdog_state_applied` for more than a few minutes: an action keeps failing.
- `increase(wattdog_parquet_write_errors_total[1h]) > 0`: the history has gaps.

## Scrape example (Grafana Alloy)

```hcl
prometheus.scrape "wattdog" {
  targets = [
    { __address__ = "127.0.0.1:9107", job = "wattdog" },
  ]
  scrape_interval = "15s"
  forward_to      = [prometheus.remote_write.cloud.receiver]
}
```

## Sample output

From a running daemon with one configured state and no PowerMon in range yet:

```text
wattdog_build_info{version="0.1.0"} 1
wattdog_up 1
wattdog_ble_scanner_running 1
wattdog_observations_received_total 0
wattdog_devices_seen 0
wattdog_state_desired{name="rpi_power"} -1
wattdog_state_applied{name="rpi_power"} -1
wattdog_state_default{name="rpi_power"} 1
wattdog_action_sequence_step{name="rpi_power"} -1
```
