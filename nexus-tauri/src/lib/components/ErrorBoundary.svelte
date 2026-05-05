<script lang="ts">
  import { showToast } from '../stores/app';

  let error: string | null = null;

  // Svelte doesn't have React-style error boundaries natively,
  // but we can catch unhandled promise rejections globally
  function handleError(e: PromiseRejectionEvent | ErrorEvent) {
    const msg = 'reason' in e
      ? String((e as PromiseRejectionEvent).reason)
      : (e as ErrorEvent).message;
    error = msg;
    showToast(`⚠ ${msg.length > 80 ? msg.slice(0, 77) + '…' : msg}`);
  }
</script>

<svelte:window
  on:unhandledrejection={handleError}
  on:error={handleError}
/>

{#if error}
  <div class="error-bar" role="alert">
    <span class="error-icon">⚠</span>
    <span class="error-msg">{error}</span>
    <button class="dismiss" on:click={() => error = null}>✕</button>
  </div>
{/if}

<style>
  .error-bar {
    position: fixed; bottom: 60px; left: 50%; transform: translateX(-50%);
    background: var(--error); color: white;
    padding: 8px 16px; border-radius: 8px;
    display: flex; align-items: center; gap: 8px;
    font-size: 13px; max-width: 600px;
    z-index: 1000; box-shadow: 0 4px 12px rgba(0,0,0,0.3);
  }
  .error-icon { font-size: 16px; }
  .error-msg { flex: 1; word-break: break-word; }
  .dismiss {
    background: none; border: none; color: white;
    font-size: 16px; cursor: pointer; opacity: 0.8;
    padding: 0 4px;
  }
  .dismiss:hover { opacity: 1; }
</style>
