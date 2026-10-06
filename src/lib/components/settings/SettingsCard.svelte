<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Title text already resolved by the caller, not the key itself. */
    title: string;
    /** Optional Reset button label; omitted when the card has no reset. */
    resetLabel?: string;
    /** Optional header actions (Undo, …) rendered before Reset. */
    actions?: Snippet;
    onreset?: () => void;
    children: Snippet;
  }

  let { title, resetLabel, actions, onreset, children }: Props = $props();
</script>

<!-- #750: the repeated `.card pane-card` shell every Settings section shares —
     section header plus optional Reset, body left to the caller. -->
<section class="card pane-card">
  <header class="section-header">
    <h2>{title}</h2>
    {#if resetLabel !== undefined && onreset !== undefined}
      <span class="section-actions">
        {#if actions !== undefined}
          {@render actions()}
        {/if}
        <button type="button" class="btn-link" onclick={onreset}>{resetLabel}</button>
      </span>
    {/if}
  </header>
  {@render children()}
</section>
<style>
  /* #981: the rules card header holds two link actions (Undo, Reset to
     default) on its trailing edge, so they need their own row rather than
     being flung apart by the header's `space-between`. */
  .section-actions {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
</style>
