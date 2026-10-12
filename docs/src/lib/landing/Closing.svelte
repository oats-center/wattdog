<script>
  import { asset, resolve } from '$app/paths'
  import FamilyFooter from '#lib/family/FamilyFooter.svelte'
</script>

<section class="cta">
  <div class="inner">
    <h2>Next steps</h2>
    <p>The quick start covers installing wattdog, writing a config for a 12 V LiFePO4 battery, validating it, and testing it with <code>--dry-run</code>.</p>
    <div class="actions">
      <a class="btn primary" href={resolve('/docs/getting-started/quickstart/')}>Quick start →</a>
      <a class="btn" href={resolve('/docs/concepts/thresholds/')}>How thresholds work</a>
    </div>
  </div>
</section>

<FamilyFooter
  current="wattdog"
  logo={asset('/logo.svg')}
  links={[
    { label: 'Docs', href: resolve('/docs/') },
    { label: 'Quick start', href: resolve('/docs/getting-started/quickstart/') },
    { label: 'Configuration', href: resolve('/docs/reference/config/') },
    { label: 'GitHub', href: 'https://github.com/oats-center/wattdog' },
  ]}
/>

<style>
  .cta { max-width: 76rem; margin: 6rem auto 0; padding: 0 1.5rem; }
  .inner {
    position: relative;
    padding: 3.5rem 2rem;
    border-radius: 1.25rem;
    text-align: center;
    color: var(--on-ink);
    background:
      url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='240' height='64'%3E%3Cpath d='M0 40h96l8-16 10 34 10-46 9 28h107' fill='none' stroke='%23b28d45' stroke-width='1.5' stroke-linejoin='round' opacity='.3'/%3E%3C/svg%3E") repeat-x 0 85%,
      repeating-linear-gradient(172deg, transparent 0 26px, rgb(255 255 255 / 0.03) 26px 27px),
      radial-gradient(80% 120% at 85% 0%, color-mix(in srgb, var(--warm) 35%, transparent), transparent 60%),
      var(--ink);
    overflow: hidden;
  }
  h2 { margin: 0; font-size: clamp(1.7rem, 3.5vw, 2.4rem); letter-spacing: -0.02em; border: 0; padding: 0; color: #fff; }
  p { margin: 0.8rem auto 2rem; max-width: 34rem; color: var(--on-ink-muted); font-size: 1.05rem; }
  .actions { display: flex; flex-wrap: wrap; justify-content: center; gap: 0.75rem; }
  .btn {
    padding: 0.7rem 1.3rem;
    border-radius: 0.6rem;
    font-weight: 600;
    text-decoration: none;
    color: var(--on-ink);
    border: 1.5px solid rgb(255 255 255 / 0.25);
  }
  .btn:hover { border-color: var(--warm); }
  .btn.primary { background: var(--warm); border-color: var(--warm); color: var(--warm-ink); }
  .btn.primary:hover { background: color-mix(in srgb, var(--warm) 85%, white); }

</style>
