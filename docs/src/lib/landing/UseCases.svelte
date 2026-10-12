<script>
  import { resolve } from '$app/paths'
  import { IconifyIcon } from '@sveltepress/theme-default/components'

  const cases = [
    { icon: 'server-bolt', name: 'Battery-backed Pi and PiKVM', to: '/docs/guides/pikvm-shutdown/', body: 'Shut the host down through PiKVM, confirm it is off, and open the load relay before the battery is drained. Restore power after the voltage recovers.', tags: ['LiFePO4', 'PiKVM', 'ATX'] },
    { icon: 'solar-panel', name: 'Solar and off-grid', to: '/docs/getting-started/quickstart/', body: 'Turn off non-critical loads such as lighting, secondary radios, or heaters when the battery bank voltage drops, and back on after it recovers.', tags: ['load shedding', 'hold_for'] },
    { icon: 'antenna', name: 'Remote sites', to: '/docs/guides/querying-data/', body: 'Trigger alerts or failover when power readings cross thresholds at an unattended site. Local Parquet files keep the history during uplink outages.', tags: ['Prometheus', 'Parquet', 'DuckDB'] },
    { icon: 'flask', name: 'Test benches', to: '/docs/reference/config/', body: 'Cut power to equipment under test when its current or power draw exceeds a limit, and restore it by threshold or manually.', tags: ['current_amps', 'power_watts'] },
  ]
</script>

<div class="cases">
  {#each cases as c}
    <a class="card" href={resolve(c.to)}>
      <span class="icon"><IconifyIcon collection="tabler" name={c.icon} /></span>
      <h3>{c.name}</h3>
      <p>{c.body}</p>
      <ul>{#each c.tags as t}<li>{t}</li>{/each}</ul>
    </a>
  {/each}
</div>

<style>
  .cases { display: grid; gap: 1rem; grid-template-columns: repeat(auto-fit, minmax(17rem, 1fr)); }
  .card {
    display: flex;
    flex-direction: column;
    padding: 1.5rem;
    border-radius: 1rem;
    text-decoration: none;
    color: inherit;
    border: 1px solid color-mix(in srgb, currentColor 12%, transparent);
    transition: border-color 0.2s, transform 0.2s;
  }
  .card:hover { border-color: var(--warm); transform: translateY(-3px); }
  .icon { font-size: 2rem; color: var(--warm); }
  h3 { margin: 1rem 0 0.4rem; font-size: 1.3rem; }
  p { margin: 0; flex: 1; line-height: 1.6; font-size: 0.95rem; color: color-mix(in srgb, currentColor 70%, transparent); }
  ul { list-style: none; margin: 1.2rem 0 0; padding: 0; display: flex; flex-wrap: wrap; gap: 0.4rem; }
  li {
    padding: 0.2rem 0.6rem;
    border-radius: 999px;
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--brand);
    background: color-mix(in srgb, var(--brand) 12%, transparent);
  }
</style>
