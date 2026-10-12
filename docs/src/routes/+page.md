---
title: wattdog
description: wattdog listens to Thornwave PowerMon battery monitors over Bluetooth, records every reading to Parquet, and switches loads over HTTP when power crosses a threshold.
layout: false
---

<script>
  import Hero from '#lib/landing/Hero.svelte'
  import Section from '#lib/landing/Section.svelte'
  import Bento from '#lib/landing/Bento.svelte'
  import Timeline from '#lib/landing/Timeline.svelte'
  import UseCases from '#lib/landing/UseCases.svelte'
  import Closing from '#lib/landing/Closing.svelte'
</script>

<div class="family-page">

<Hero />

<Section eyebrow="Features" title="What wattdog does" lede="wattdog reads PowerMon Bluetooth advertisements, keeps every reading, and switches loads over HTTP when a measurement stays past a threshold for a set time.">
<Bento />
</Section>

<Section eyebrow="Example" title="Low-battery shutdown and recovery" lede="The sequence from the PiKVM guide, step by step. Each step corresponds to a config setting and is reported in the metrics." link={{ to: '/docs/guides/pikvm-shutdown/', label: 'PiKVM graceful shutdown guide' }}>
<Timeline />
</Section>

<Section split eyebrow="Configuration" title="Configuration file" lede="One TOML file. Each [[states]] block controls one output from one PowerMon measurement, with an on and an off threshold and how long each must hold. A transition calls one URL or runs a sequence of steps." link={{ to: '/docs/reference/config/', label: 'Configuration reference' }}>
<Tabs activeName="config.toml">
<TabPanel name="config.toml">

```toml
[data]
dir = "/var/lib/wattdog"

[http]
method = "POST"

[[states]]
name = "rpi_power"
serial = "12345678"
field = "voltage1_volts"
default_state = "on"
stale_after = "45s"

[states.on]
op = ">="
value = 13.1
duration = "10m"
url = "http://relay.local/on"

[states.off]
op = "<="
value = 11.2
duration = "2m"
url = "http://relay.local/off"
```

</TabPanel>
<TabPanel name="Shutdown sequence">

```toml
# Ask the host to shut down, confirm it did, then cut power.
[states.off]
op = "<="
value = 11.2
duration = "2m"
hold_for = "30m"

[[states.off.actions]]
type = "request"
url = "https://admin:pw@pikvm/api/atx/power?action=off&wait=1"

[[states.off.actions]]
type = "wait_for_json"
url = "https://admin:pw@pikvm/api/atx"
pointer = "/result/leds/power"
expected = false
poll_every = "5s"
timeout = "2m"
on_timeout = "continue"

[[states.off.actions]]
type = "request"
url = "https://admin:pw@pikvm/api/gpio/switch?channel=load&state=0"
```

</TabPanel>
</Tabs>
</Section>

<Section split eyebrow="Data" title="Recorded readings" lede="Every advertisement is written as a row to hourly or daily Parquet files under the data directory. Metrics show only the latest values; the Parquet files are the full history and can be queried with DuckDB, Polars, or similar tools." link={{ to: '/docs/guides/querying-data/', label: 'Query the data' }}>
<Tabs activeName="DuckDB">
<TabPanel name="DuckDB">

```sql
SELECT
  date_trunc('minute', observed_at_utc) AS minute,
  serial,
  avg(voltage1_volts) AS avg_v1,
  avg(current_amps)   AS avg_current,
  avg(power_watts)    AS avg_power
FROM read_parquet('/var/lib/wattdog/samples/**/*.parquet')
GROUP BY minute, serial
ORDER BY minute, serial;
```

</TabPanel>
<TabPanel name="Layout">

```text
/var/lib/wattdog/samples/
  date=2026-10-12/
    hour=13/part-20261012T130000Z.parquet
    hour=14/part-20261012T140000Z.parquet.inprogress
```

</TabPanel>
</Tabs>
</Section>

<Section eyebrow="Use cases" title="Typical deployments" lede="The same daemon and config format are used in each case.">
<UseCases />
</Section>

<Closing />

</div>
