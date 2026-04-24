<script lang="ts">
  import { currentView } from '$lib/stores/app';

  const BUILD = import.meta.env.__APP_BUILD__ || '2.3.6.unknown';

  function goBack() {
    currentView.set('dashboard');
  }
</script>

<div class="about">
  <div class="about-card">
    <div class="logo">🎵</div>
    <h1>PresenceJam</h1>
    <p class="version">{BUILD}</p>
    <p class="description">
      Syncs your Spotify playback to Microsoft Teams status automatically.
    </p>
    <div class="links">
      <a href="https://github.com/Carme99/PresenceJam-Desktop" target="_blank" rel="noopener">
        View on GitHub
      </a>
      <span class="separator">•</span>
      <a href="https://github.com/Carme99/PresenceJam-Desktop/releases" target="_blank" rel="noopener">
        Releases
      </a>
    </div>
    <button class="close-btn" onclick={goBack}>Back to Dashboard</button>
  </div>
</div>

<style>
  .about {
    height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-primary);
    padding: 20px;
  }
  .about-card {
    background: var(--bg-surface);
    border: 1px solid var(--border-color);
    border-radius: 12px;
    padding: 40px;
    text-align: center;
    max-width: 400px;
    width: 100%;
  }
  .logo {
    font-size: 48px;
    margin-bottom: 16px;
  }
  h1 {
    font-size: 24px;
    font-weight: 600;
    margin-bottom: 8px;
  }
  .version {
    font-size: 12px;
    color: var(--text-secondary);
    margin-bottom: 16px;
  }
  .description {
    color: var(--text-secondary);
    font-size: 14px;
    line-height: 1.5;
    margin-bottom: 20px;
  }
  .links {
    display: flex;
    justify-content: center;
    gap: 8px;
    margin-bottom: 24px;
    font-size: 13px;
  }
  .links a {
    color: var(--color-accent);
    text-decoration: none;
  }
  .links a:hover {
    text-decoration: underline;
  }
  .separator {
    color: var(--text-secondary);
  }
  .close-btn {
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: 8px;
    padding: 10px 20px;
    font-size: 14px;
    cursor: pointer;
    color: var(--text-primary);
  }
  .close-btn:hover {
    background: var(--border-color);
  }
</style>