---
title: "Design"
description: "How wattdog is put together: the SDK boundary, the queues, and its storage and metric policies."
---

wattdog is a small daemon that turns Bluetooth advertisements into actions:

```text
Thornwave advertisements
  → C-compatible C++ facade
  → bindgen Rust FFI
  → scanner callback
  → bounded sample queue
  → metrics / latest-value processor
  → threshold state engine  → HTTP action client
  → Parquet writer queue    → hourly or daily Parquet files
```

The metrics server runs independently of the scanner and stays up even if the scanner fails to start or stalls, so a
broken Bluetooth stack is visible in `/metrics` instead of being silent.

## The SDK boundary

PowerMon advertisements are decoded by Thornwave's closed-source C++ SDK. Rust doesn't bind to its
C++ classes. A small facade (`ffi/thornwave_scan.*`) exposes only:

- an opaque scanner handle;
- a C-compatible advertisement struct;
- a C callback;
- start and stop functions;
- a last-error accessor.

The facade catches every C++ exception and returns it as an integer failure.

## The callback path

The SDK calls back on its own thread. The callback copies the advertisement into Rust-owned data
and tries a **non-blocking** send into a bounded queue of 16,384 samples. It never writes to disk,
sends HTTP, or waits. If the queue is full, it drops the observation and increments
`wattdog_observations_dropped_total`.

## State and actions

Each state watches one device serial and one measurement. The engine ticks once a second. Fresh
samples advance the on and off timers. Stale samples pause new decisions, but work already in
progress continues. The exact rules are in [How thresholds work](/docs/concepts/thresholds/).

A transition is one URL or an ordered sequence of requests, delays, and JSON polling waits. Delays
and the gaps between polls are timers, so they don't hold anything up. Requests themselves are
sent one at a time, so a slow endpoint can delay other states' requests by up to
`[http].timeout`. `--dry-run` treats every request and poll as an immediate success.

The applied state changes only after a sequence's final step. Its `hold_for` dwell then runs before
the opposite threshold starts timing. Delivery is at least once and progress isn't persisted, so
actuator endpoints must be idempotent.

## Storage

Samples go through a second bounded queue (16,384) to a background writer. The writer batches
rows into Arrow record batches and writes them under `[data].dir/samples`. The active file is
`*.parquet.inprogress`. At a roll boundary or on graceful shutdown (SIGINT or SIGTERM), wattdog
closes it and renames it to `*.parquet`.

The daemon never deletes files. Retention is up to you (see [Query the
data](/docs/guides/querying-data/#Retention)).

## Metrics

Metrics show daemon health and the **latest** device and state values. They aren't the durable
record. The Parquet files are, with one row per observed advertisement.

Device metrics are labeled by `serial` only. Identity fields appear only on `wattdog_device_info`.
Action URLs are never logged in full and never used as labels, because they can hold credentials.
