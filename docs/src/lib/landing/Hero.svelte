<script>
  import { resolve } from '$app/paths'

  // Real output from wattdog (timestamps trimmed). No PowerMon was in range.
  const views = [
    {
      id: 'check',
      lines: [
        ['cmd', 'wattdog --config config.toml --check-config'],
        ['log', 'INFO', 'loaded wattdog config', 'config=config.toml'],
        ['ok', 'INFO', 'config is valid', 'config=config.toml'],
        ['blank'],
        ['cmd', 'wattdog --config bad.toml --check-config'],
        ['err', 'Error: state rpi_power on/off thresholds overlap'],
      ],
    },
    {
      id: 'dry-run',
      lines: [
        ['cmd', 'wattdog --config config.toml --dry-run'],
        ['log', 'INFO', 'starting wattdog', 'dry_run=true'],
        ['log', 'INFO', 'loaded Thornwave SDK', 'library_version_bcd=0x0121'],
        ['log', 'INFO', 'metrics HTTP server listening', 'addr=127.0.0.1:9107'],
        ['ok', 'INFO', 'started Thornwave BLE scan', ''],
        ['blank'],
        ['note', '# In dry-run mode, actions are logged but not sent.'],
      ],
    },
    {
      id: 'metrics',
      lines: [
        ['cmd', 'curl -s localhost:9107/metrics | grep -v "^#"'],
        ['kv', 'wattdog_up ', '1'],
        ['kv', 'wattdog_ble_scanner_running ', '1'],
        ['kv', 'wattdog_state_stale{name="rpi_power"} ', '0'],
        ['kv', 'wattdog_state_desired{name="rpi_power"} ', '-1'],
        ['kv', 'wattdog_state_applied{name="rpi_power"} ', '-1'],
        ['kv', 'wattdog_action_sequence_step{name="rpi_power"} ', '-1'],
      ],
    },
  ]

  let active = $state(0)
  const v = $derived(views[active])
</script>

<section class="hero">
  <div class="field" aria-hidden="true"></div>

  <div class="copy">
    <p class="kicker"><span>PowerMon BLE</span> + <span>thresholds</span> + <span>HTTP</span></p>
    <h1>wattdog</h1>
    <p class="tagline">A watchdog daemon for Thornwave PowerMon battery monitors.</p>
    <p class="lead">
      It receives PowerMon Bluetooth Low Energy advertisements, writes every reading to local
      Parquet files, and calls HTTP endpoints when a voltage, current, or power reading stays past
      a configured threshold. It serves health and Prometheus metrics. It runs on Raspberry Pi,
      PiKVM, and other Linux hosts with BlueZ.
    </p>
    <div class="actions">
      <a class="btn primary" href={resolve('/docs/getting-started/quickstart/')}>Get started <span aria-hidden="true">→</span></a>
      <a class="btn" href={resolve('/docs/')}>Read the docs</a>
      <a class="btn ghost" href="https://github.com/oats-center/wattdog">GitHub</a>
    </div>
  </div>

  <div class="console">
    <div class="tabs" role="tablist" aria-label="Example output">
      {#each views as view, i}
        <button role="tab" aria-selected={i === active} class:on={i === active} onclick={() => (active = i)}>
          {view.id}
        </button>
      {/each}
    </div>
    <pre aria-live="polite">{#each v.lines as l}{#if l[0] === 'cmd'}<span class="p">$</span> {l[1]}
{:else if l[0] === 'kv'}{l[1]}<b>{l[2]}</b>
{:else if l[0] === 'log' || l[0] === 'ok'}<span class="lvl">{l[1]}</span> <span class={l[0] === 'ok' ? 'ok' : ''}>{l[2]}</span> <span class="dim">{l[3]}</span>
{:else if l[0] === 'pass'}<span class="ok">{l[1]}</span>
{:else if l[0] === 'err'}<span class="err">{l[1]}</span>
{:else if l[0] === 'note'}<span class="note">{l[1]}</span>
{:else if l[0] === 'blank'}
{:else}<span class="dim">{l[1]}</span>
{/if}{/each}</pre>
    <p class="caption">Real output. No PowerMon in range.</p>
  </div>
</section>

<style>
  .hero {
    position: relative;
    display: grid;
    gap: 3rem;
    align-items: center;
    max-width: 76rem;
    margin: 0 auto;
    padding: 4.5rem 1.5rem 3rem;
  }
  @media (min-width: 960px) {
    .hero { grid-template-columns: 1.05fr 1fr; padding-top: 6rem; }
  }

  /* wattdog motif: a pulse waveform running over field rows */
  .field {
    position: absolute;
    inset: -2rem -50vw 0;
    z-index: -1;
    background:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='240' height='64'%3E%3Cpath d='M0 40h96l8-16 10 34 10-46 9 28h107' fill='none' stroke='%23b28d45' stroke-width='1.5' stroke-linejoin='round' opacity='.4'/%3E%3C/svg%3E") repeat-x 0 9%,
      radial-gradient(60% 70% at 75% 30%, color-mix(in srgb, var(--warm) 16%, transparent), transparent 70%),
      repeating-linear-gradient(172deg, transparent 0 26px, color-mix(in srgb, var(--brand) 7%, transparent) 26px 27px);
    mask-image: linear-gradient(to bottom, black 40%, transparent);
  }

  .kicker {
    margin: 0 0 1rem;
    font-family: var(--svp-code-font);
    font-size: 0.8rem;
    color: color-mix(in srgb, currentColor 55%, transparent);
  }
  .kicker span { color: var(--brand); font-weight: 600; }

  h1 {
    margin: 0;
    font-size: clamp(2.4rem, 5vw, 3.6rem);
    line-height: 1;
    font-weight: 750;
    letter-spacing: -0.035em;
  }
  .tagline {
    margin: 0.6rem 0 0;
    font-family: var(--font-display);
    font-size: clamp(1.25rem, 2.2vw, 1.6rem);
    font-weight: 600;
    line-height: 1.25;
    color: color-mix(in srgb, currentColor 80%, transparent);
  }

  .lead {
    margin: 1.5rem 0 0;
    max-width: 36rem;
    font-size: 1.15rem;
    line-height: 1.65;
    color: color-mix(in srgb, currentColor 72%, transparent);
  }

  .actions { display: flex; flex-wrap: wrap; gap: 0.75rem; margin-top: 2rem; }
  .btn {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.7rem 1.3rem;
    border-radius: 0.6rem;
    font-weight: 600;
    text-decoration: none;
    color: inherit;
    border: 1.5px solid color-mix(in srgb, currentColor 22%, transparent);
    transition: transform 0.15s, border-color 0.15s, background 0.15s;
  }
  .btn:hover { transform: translateY(-1px); border-color: var(--brand); }
  .btn.primary { background: var(--brand); border-color: var(--brand); color: var(--on-brand); }
  .btn.primary:hover { background: var(--brand-deep); }
  .btn.ghost { border-color: transparent; }

  .console {
    min-width: 0;
    border-radius: 1rem;
    background: var(--console-bg);
    border: 1px solid var(--console-line);
    box-shadow: 0 40px 80px -30px rgb(10 30 15 / 0.55);
    overflow: hidden;
  }
  .tabs { display: flex; background: var(--console-bar); border-bottom: 1px solid var(--console-line); }
  .tabs button {
    flex: 1;
    padding: 0.65rem 0.5rem;
    background: none;
    border: 0;
    border-bottom: 2px solid transparent;
    color: var(--console-dim);
    font: 500 0.78rem var(--svp-code-font);
    cursor: pointer;
  }
  .tabs button.on { color: var(--on-ink); border-bottom-color: var(--warm); background: var(--console-bg); }
  pre {
    margin: 0;
    padding: 1.1rem 1.25rem 0.5rem;
    min-height: 15.5rem;
    font: 0.78rem/1.65 var(--svp-code-font);
    color: var(--console-text);
    background: none;
    overflow-x: auto;
  }
  pre b { color: #fff; font-weight: 600; }
  .p { color: var(--warm); }
  .lvl { color: #6fb3d0; }
  .ok { color: #6fd08c; }
  .err { color: #f08a7a; }
  .dim { color: var(--console-dim); }
  .note { color: var(--warm); }
  .caption {
    margin: 0;
    padding: 0.4rem 1.25rem 0.9rem;
    font-size: 0.75rem;
    color: var(--console-dim);
  }
</style>
