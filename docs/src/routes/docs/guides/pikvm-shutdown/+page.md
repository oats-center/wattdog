---
title: "PiKVM graceful shutdown"
description: "Shut the host down through PiKVM, wait until it is really off, then cut the load relay."
---

Cutting power to a running computer risks corrupting its disk. With an action sequence, wattdog
can do what a careful operator would:

1. Ask PiKVM to press the host's ATX power button.
2. Poll PiKVM until the host's power LED goes dark.
3. Open the load relay.

It reverses the steps when the battery recovers.

## The config

This replaces the single `url` in each transition of the [quick
start](/docs/getting-started/quickstart/) config with an `actions` sequence.

```toml
[states.on]
op = ">="
value = 13.1
duration = "10m"
hold_for = "5m"

[[states.on.actions]]          # 1. power the load back on
type = "request"
method = "POST"
url = "https://admin:password@pikvm/api/gpio/switch?channel=load&state=1&wait=1"

[[states.on.actions]]          # 2. let the supply settle
type = "delay"
duration = "5s"

[[states.on.actions]]          # 3. press the host's power button
type = "request"
method = "POST"
url = "https://admin:password@pikvm/api/atx/power?action=on&wait=1"

[states.off]
op = "<="
value = 11.2
duration = "2m"
hold_for = "30m"

[[states.off.actions]]         # 1. ask the host to shut down
type = "request"
method = "POST"
url = "https://admin:password@pikvm/api/atx/power?action=off&wait=1"

[[states.off.actions]]         # 2. wait until its power LED is off
type = "wait_for_json"
url = "https://admin:password@pikvm/api/atx"
pointer = "/result/leds/power"
expected = false
poll_every = "5s"
timeout = "2m"
on_timeout = "continue"

[[states.off.actions]]         # 3. remove the load
type = "request"
method = "POST"
url = "https://admin:password@pikvm/api/gpio/switch?channel=load&state=0&wait=1"
```

Replace `load` with the GPIO channel you configured in PiKVM.

## Why it's built this way

- **`wait=1` isn't enough on its own.** It makes PiKVM wait for the button press to finish, not
  for the operating system to finish shutting down. The `wait_for_json` step checks that the host
  is actually off.
- **`on_timeout = "continue"` protects the battery.** If the host never powers off, wattdog still
  cuts the relay after two minutes. Use `retry` only when keeping the host up matters more than
  the battery.
- **`hold_for = "30m"` prevents flapping.** After the load is cut, wattdog keeps it off for at
  least 30 minutes. Only then does it start timing the 10 minutes above 13.1 V that it needs before
  turning the load back on.
- **The steps are idempotent.** Each request sets a state (`state=0`, `action=off`) rather than
  toggling it. wattdog delivers HTTP requests *at least once*: if a response is lost, it repeats
  the step. After a restart, it begins the sequence again from the first step.

## Credentials

wattdog takes the `user:password@` part of a URL and sends it as HTTP basic authentication. It
never logs full action URLs or uses them as metric labels. The config file still holds the
password, so keep it at mode `0600`. If PiKVM uses its default self-signed certificate,
either install a trusted one or set `[http].allow_invalid_certs = true`.

## Wiring

Keep PiKVM and the device running wattdog powered **independently** of the relay-controlled load.
If wattdog cuts its own power, nothing can turn the load back on.

## Watching it work

```bash
curl -s http://127.0.0.1:9107/metrics | grep -E 'state_(desired|applied)|sequence_step|wait_timeouts'
```

`wattdog_action_sequence_step` shows the step in progress (`-1` when idle).
`wattdog_action_wait_timeouts_total` counts the times the host didn't power off in time.
