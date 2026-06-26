# Design

`wattdog` is a small BLE-to-action daemon:

```text
Thornwave advertisements
  -> C-compatible C++ facade
  -> bindgen Rust FFI
  -> scanner callback
  -> bounded sample channel
  -> metrics/latest-value processor
  -> threshold state engine
  -> HTTP action client
  -> Parquet writer queue
  -> hourly/daily Parquet files
```

The metrics endpoint stays available even if scanner startup fails or stalls. Action URLs are treated as secrets: they are not exposed in metrics and must not be logged in full.

## SDK boundary

Rust does not bind directly to Thornwave C++ classes. The `ffi/thornwave_scan.*` facade exposes only:

- opaque scanner handle
- C-compatible advertisement struct
- C callback
- start/stop functions
- last-error accessor

All C++ exceptions are caught at the facade boundary and returned as integer failures.

## Callback policy

The Thornwave callback path copies the advertisement into Rust-owned data and attempts a non-blocking send into a bounded channel. It never writes to disk, sends HTTP, or awaits.

If the queue is full, the observation is dropped and `wattdog_observations_dropped_total` increments.

## State and action policy

Each configured state selects one device serial and one measurement field. Fresh samples advance ON/OFF threshold timers; stale samples pause new decisions but keep already-pending retries alive.

HTTP actions are sent only when a transition is due. `2xx` responses and `--dry-run` count as success and update the applied state. Failures keep the old applied state and retry with capped backoff.

## Storage policy

Parquet files are written under `config.data.dir/samples`. Active files use `.parquet.inprogress`; on roll boundary or graceful shutdown, the writer closes the Arrow writer and renames the file to `.parquet`.

The daemon does not delete old files. Use systemd timers, `tmpfiles.d`, or another external cleanup mechanism for completed files.

## Metric policy

Prometheus metrics represent daemon health and latest device/state values. They are not the durable record. The durable record is one Parquet row per observed advertisement.

Device metrics use `serial` as the primary label. Identity fields appear only on `wattdog_device_info`. Action URLs are never metric labels.
