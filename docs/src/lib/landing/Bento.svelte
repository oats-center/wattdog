<script>
  import { resolve } from '$app/paths'
  import { IconifyIcon } from '@sveltepress/theme-default/components'

  const tiles = [
    {
      icon: 'bluetooth',
      title: 'Listens, never pairs',
      body: 'PowerMon devices already broadcast their readings. wattdog hears every one in range through BlueZ, with nothing to pair and no connection to drop.',
      to: '/docs/concepts/design/',
      size: 'wide',
    },
    {
      icon: 'hourglass',
      title: 'Waits until it’s sure',
      body: 'A threshold fires only after holding continuously for its full duration. One dip under load resets the clock.',
      to: '/docs/concepts/thresholds/',
    },
    {
      icon: 'list-numbers',
      title: 'Acts in order',
      body: 'Call one URL, or run a sequence: press the power button, wait for the LED to go dark, then cut the relay.',
      to: '/docs/guides/pikvm-shutdown/',
    },
    {
      icon: 'database',
      title: 'Keeps every reading',
      body: 'Each advertisement becomes a row in an hourly or daily Parquet file. When the network was down, the history is still there for DuckDB or Polars.',
      to: '/docs/guides/querying-data/',
    },
    {
      icon: 'chart-line',
      title: 'Observable',
      body: '/healthz and Prometheus /metrics stay up even when Bluetooth doesn’t start, so a broken radio isn’t a silent one.',
      to: '/docs/reference/metrics/',
    },
    {
      icon: 'shield-check',
      title: 'Safe to try',
      body: '--check-config rejects overlapping thresholds and bad URLs before anything runs. --dry-run runs the whole pipeline and sends no HTTP requests.',
      to: '/docs/getting-started/quickstart/',
      size: 'full',
    },
  ]
</script>

<div class="bento">
  {#each tiles as t}
    <a class="tile {t.size ?? ''}" href={resolve(t.to)}>
      <span class="icon"><IconifyIcon collection="tabler" name={t.icon} /></span>
      <h3>{t.title}</h3>
      <p>{t.body}</p>
    </a>
  {/each}
</div>

<style>
  .bento {
    display: grid;
    gap: 1rem;
    grid-template-columns: repeat(auto-fit, minmax(16rem, 1fr));
  }
  @media (min-width: 960px) {
    .bento { grid-template-columns: repeat(3, 1fr); grid-auto-flow: dense; }
    .wide { grid-column: span 2; }
    .full { grid-column: 1 / -1; }
  }
  .tile {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    padding: 1.5rem;
    border-radius: 1rem;
    text-decoration: none;
    color: inherit;
    background: color-mix(in srgb, var(--brand) 5%, transparent);
    border: 1px solid color-mix(in srgb, var(--brand) 18%, transparent);
    transition: border-color 0.2s, background 0.2s;
    overflow: hidden;
  }
  .tile:hover {
    border-color: var(--brand);
    background: color-mix(in srgb, var(--brand) 9%, transparent);
  }
  .tile::after {
    content: '→';
    position: absolute;
    top: 1.3rem;
    right: 1.4rem;
    opacity: 0;
    color: var(--warm);
    transition: opacity 0.2s, transform 0.2s;
  }
  .tile:hover::after { opacity: 1; transform: translateX(3px); }
  .full {
    background:
      linear-gradient(160deg, color-mix(in srgb, var(--warm) 14%, transparent), transparent 60%),
      color-mix(in srgb, var(--brand) 5%, transparent);
  }
  .icon {
    display: inline-flex;
    align-self: flex-start;
    padding: 0.6rem;
    border-radius: 0.7rem;
    font-size: 1.4rem;
    color: var(--on-brand);
    background: var(--brand);
  }
  h3 { margin: 0.8rem 0 0; font-size: 1.2rem; font-weight: 650; }
  p { margin: 0; line-height: 1.6; font-size: 0.95rem; color: color-mix(in srgb, currentColor 70%, transparent); }
</style>
