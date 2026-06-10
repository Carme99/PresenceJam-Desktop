<script>
  import '../app.css';
  import { devLog } from '$lib/utils/dev';
  devLog(`[LAYOUT] PresenceJam build: ${import.meta.env.VITE_APP_BUILD ?? 'dev build'}`);
</script>

<slot />
