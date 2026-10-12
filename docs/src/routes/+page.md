---
title: A watchdog for every watt
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

<Section eyebrow="Why wattdog" title="Batteries don’t warn you twice" lede="A battery-powered computer that runs flat can corrupt its disk, and nobody may notice until someone drives out to the site. wattdog watches the battery for you, and it only acts on readings it trusts.">
<Bento />
</Section>

<Section eyebrow="A night on battery" title="Patient on the way down, careful on the way up" lede="This is the PiKVM example from the guides. Every step is a line of config, and every step shows up in the metrics." link={{ to: '/docs/guides/pikvm-shutdown/', label: 'Read the PiKVM guide' }}>
<Timeline />
</Section>

<Section split eyebrow="Configuration" title="One file, one block per output" lede="Name the PowerMon, pick a measurement, and set two thresholds with how long each must hold. A transition can be one URL, or a sequence." link={{ to: '/docs/reference/config/', label: 'Configuration reference' }}>
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

<Section split eyebrow="Keep the record" title="Every reading, ready to query" lede="Metrics show what's happening now. The Parquet files record everything that happened. Point DuckDB at the directory and check your thresholds against last month before you trust them." link={{ to: '/docs/guides/querying-data/', label: 'Query the data' }}>
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

<Section eyebrow="Built for the field" title="Wherever a battery keeps something alive" lede="The same daemon and config format, whether it guards one Raspberry Pi or a cabinet at a remote site.">
<UseCases />
</Section>

<Closing />

</div>
