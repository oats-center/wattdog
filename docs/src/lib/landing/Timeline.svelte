<script>
  // The PiKVM example from the guide: a battery running down, and recovering.
  const steps = [
    { state: '≤ 11.2 V', note: 'The battery sags under load. The off timer starts, and any reading above 11.2 V resets it.' },
    { state: 'held 2 min', note: 'The voltage stayed low for two continuous minutes. The off sequence begins and runs to the end.', hold: true },
    { state: 'host off', note: 'PiKVM presses the ATX power button. wattdog polls the power LED until the host is really off.' },
    { state: 'relay open', note: 'The load is cut. A 30-minute dwell begins, so a bounce in voltage can’t flip it straight back.', hold: true },
    { state: '≥ 13.1 V', note: 'The battery has recovered and stays above 13.1 V for ten minutes. The load and the host come back on.', done: true },
  ]
</script>

<ol class="flow">
  {#each steps as s, i}
    <li class:hold={s.hold} class:done={s.done}>
      <span class="dot">{i + 1}</span>
      <span class="state">{s.state}</span>
      <p>{s.note}</p>
    </li>
  {/each}
</ol>

<style>
  .flow {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 1rem;
    grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr));
    position: relative;
  }
  @media (min-width: 960px) {
    .flow::before {
      content: '';
      position: absolute;
      top: 1.05rem;
      left: 1rem;
      right: 1rem;
      height: 2px;
      background: linear-gradient(90deg, var(--brand), var(--warm));
      opacity: 0.5;
    }
  }
  li { position: relative; }
  .dot {
    position: relative;
    display: inline-grid;
    place-items: center;
    width: 2.1rem;
    height: 2.1rem;
    border-radius: 50%;
    font: 700 0.85rem var(--svp-code-font);
    color: var(--on-brand);
    background: var(--brand);
    box-shadow: 0 0 0 5px var(--paper);
  }
  :global(html.dark) .dot { box-shadow: 0 0 0 5px var(--paper-dark); }
  .hold .dot { background: var(--warm); color: var(--warm-ink); }
  .done .dot { background: #2f9e57; }
  .state {
    display: block;
    margin-top: 0.9rem;
    font-size: 0.9rem;
    font-weight: 600;
    font-family: var(--svp-code-font);
  }
  .hold .state { color: var(--warm); }
  p { margin: 0.35rem 0 0; font-size: 0.9rem; line-height: 1.55; color: color-mix(in srgb, currentColor 68%, transparent); }
</style>
