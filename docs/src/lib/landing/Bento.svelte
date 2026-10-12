<script>
  import { resolve } from '$app/paths'
  import { IconifyIcon } from '@sveltepress/theme-default/components'

  const tiles = [
    {
      icon: 'bluetooth',
      title: 'Passive Bluetooth scanning',
      body: 'PowerMon devices broadcast their readings in BLE advertisements. wattdog receives them through BlueZ from every device in range, without pairing or holding a connection.',
      to: '/docs/concepts/design/',
      size: 'wide',
    },
    {
      icon: 'hourglass',
      title: 'Duration-qualified thresholds',
      body: 'A transition fires only after its condition has held continuously for its configured duration. A single reading that breaks the condition resets the timer.',
      to: '/docs/concepts/thresholds/',
    },
    {
      icon: 'list-numbers',
      title: 'Action sequences',
      body: 'A transition calls one URL, or runs ordered steps: HTTP requests, delays, and polling a JSON endpoint until a value matches.',
      to: '/docs/guides/pikvm-shutdown/',
    },
    {
      icon: 'database',
      title: 'Parquet history',
      body: 'Each advertisement is stored as a row in an hourly or daily Parquet file on local disk, independent of network connectivity.',
      to: '/docs/guides/querying-data/',
    },
    {
      icon: 'chart-line',
      title: 'Health and metrics',
      body: '/healthz and Prometheus /metrics report scanner, device, state, and action status. They stay available if the Bluetooth scanner fails to start.',
      to: '/docs/reference/metrics/',
    },
    {
      icon: 'shield-check',
      title: 'Validation and dry runs',
      body: '--check-config validates a config, including threshold direction and overlap, and exits. --dry-run runs everything but treats HTTP actions as successful without sending them.',
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
