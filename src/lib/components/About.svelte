<script lang="ts">
  import { currentView } from '$lib/stores/app';
  import Logo from './Logo.svelte';

  const BUILD = import.meta.env.VITE_APP_BUILD ?? 'dev build';

  function goBack() {
    currentView.set('dashboard');
  }
</script>

<div class="about">
  <div class="card about-card">
    <div class="logo-wrap"><Logo size={72} /></div>
    <h1>PresenceJam</h1>
    <p class="version">Version {BUILD}</p>
    <p class="description">
      Pipes what you're playing on Spotify into your Microsoft Teams status — automatically.
    </p>
    <dl class="stats">
      <div>
        <dt>Status sync</dt>
        <dd>Live</dd>
      </div>
      <div>
        <dt>Auth</dt>
        <dd>PKCE / Device Code</dd>
      </div>
      <div>
        <dt>Storage</dt>
        <dd>OS keychain</dd>
      </div>
    </dl>
    <div class="links">
      <a href="https://github.com/Carme99/PresenceJam-Desktop" target="_blank" rel="noopener">
        GitHub repository
      </a>
      <span class="separator">·</span>
      <a href="https://github.com/Carme99/PresenceJam-Desktop/releases" target="_blank" rel="noopener">
        Releases
      </a>
      <span class="separator">·</span>
      <a href="https://github.com/Carme99/PresenceJam-Desktop/issues" target="_blank" rel="noopener">
        Report an issue
      </a>
    </div>
    <button class="btn-secondary" onclick={goBack}>Back to dashboard</button>
  </div>
</div>

<style>
  .about {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-base);
    padding: var(--sp-7);
  }
  .about-card {
    width: 100%;
    max-width: 420px;
    padding: var(--sp-8);
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-3);
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--r-lg);
    box-shadow: var(--shadow-2);
  }
  .logo-wrap {
    width: 88px;
    height: 88px;
    border-radius: var(--r-lg);
    background: var(--bg-elevated);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    margin-bottom: var(--sp-2);
  }
  h1 {
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  .version {
    font-size: var(--fs-xs);
    color: var(--fg-subtle);
    padding: 2px var(--sp-3);
    border: 1px solid var(--border);
    border-radius: var(--r-pill);
    background: var(--bg-elevated);
    font-variant-numeric: tabular-nums;
  }
  .description {
    color: var(--fg-muted);
    font-size: var(--fs-base);
    line-height: var(--lh-normal);
    max-width: 32ch;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--sp-2);
    width: 100%;
    margin: var(--sp-3) 0;
    padding-top: var(--sp-3);
    border-top: 1px solid var(--border);
  }
  .stats > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .stats dt {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-subtle);
    font-weight: 600;
  }
  .stats dd {
    font-size: var(--fs-sm);
    color: var(--fg);
    font-weight: 600;
  }
  .links {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: var(--sp-2);
    margin-top: var(--sp-2);
    font-size: var(--fs-sm);
  }
  .links a {
    color: var(--accent-text);
    text-decoration: none;
    font-weight: 600;
  }
  .links a:hover { color: var(--accent); text-decoration: underline; }
  .separator { color: var(--fg-subtle); }
</style>
