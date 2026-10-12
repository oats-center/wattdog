---
title: "How thresholds work"
description: "The exact rules wattdog's state engine follows: continuous duration, direction, stale data, dwell, retries, and startup."
---

Each `[[states]]` block is one on/off output, driven by one measurement from one PowerMon. The
state engine evaluates it once per second. These rules decide when it acts.

## Continuous duration

A transition fires only when its condition has held **continuously** for its full `duration`. A
single reading that breaks the condition resets the timer to zero.

With `op = "<="`, `value = 11.2`, and `duration = "2m"`, the voltage must stay at or below 11.2 V for
two full minutes. One reading of 11.3 V starts the clock over. A brief dip under load doesn't
shut anything down.

## Opposite directions, no overlap

`on` and `off` must point in opposite directions: one uses `<` or `<=`, the other `>` or `>=`. Their
ranges can't overlap, either. wattdog refuses to start otherwise:

```console
$ wattdog --config bad.toml --check-config
Error: state rpi_power on/off thresholds overlap
```

The gap between the two thresholds is the **neutral band**. In it, neither transition is timing,
and wattdog leaves the output alone.

## Stale data

If no advertisement from a state's PowerMon arrives within `stale_after`, the state is stale.
While it's stale:

- wattdog starts no new transitions, and `wattdog_state_stale` is `1`.
- A sequence that has already started runs to completion, including its retries. wattdog
  doesn't abandon work in progress because the radio went quiet.
- Duration timers are **paused, not reset**. If the condition still holds at the first fresh
  reading, the time spent stale counts toward the duration. If it doesn't hold, the timer resets as
  usual.

## Sequences are atomic

Once a transition starts, it runs to the end, even if the input goes stale or the opposite
threshold starts matching. A failed request retries in place, and earlier steps aren't repeated.
The **applied** state changes only after the last step succeeds.

Delivery is *at least once*. If an actuator handles a request but the response is lost, wattdog
sends it again. Progress is kept in memory, so after a restart the sequence begins again from its
first step. Use endpoints that **set** a state (`state=0`), never ones that toggle it.

## Retry backoff

A failed request waits before retrying, starting at `retry_initial` and doubling up to `retry_max`
(at most 5 s):

| Failure | Delay (`retry_initial = "1s"`, `retry_max = "5s"`) |
|---|---|
| 1st | 1 s |
| 2nd | 2 s |
| 3rd | 4 s |
| 4th and later | 5 s |

A success resets the backoff. A `wait_for_json` step that hasn't seen its value yet doesn't back
off. It polls again after `poll_every`.

## Dwell (`hold_for`)

When a sequence completes, its transition's `hold_for` begins. During the dwell, wattdog doesn't
time either threshold. Once the dwell ends, the opposite condition must still hold for its full
`duration`.

With `off.hold_for = "30m"` and `on.duration = "10m"`, the output stays off for **at least 40
minutes**: 30 minutes of dwell, then 10 continuous minutes above the on threshold.

## Startup and `default_state`

wattdog doesn't know the output's real state when it starts, so the applied state starts as
unknown (`-1` in metrics). The first decision drives the output to a known state:

- If the first fresh reading is in the neutral band, wattdog takes `default_state` as the desired
  state and runs its action right away.
- If the reading already satisfies a threshold, wattdog waits for that threshold's `duration`, then
  acts.

Either way, expect one action shortly after every start. Once an action has succeeded,
`default_state` is no longer used.

## Ambiguity guard

If both transitions were ever satisfied at once, wattdog would do nothing and set
`wattdog_state_ambiguous` to `1`. Because the validator rejects overlapping thresholds, a valid
config can't reach this state. It's a safety net.
