<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let title: string = 'Confirm';
  export let message: string = 'Are you sure?';
  export let confirmLabel: string = 'Confirm';
  export let destructive: boolean = false;

  const dispatch = createEventDispatcher();

  function confirm() { dispatch('confirm'); }
  function cancel() { dispatch('cancel'); }

  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') cancel();
    if (e.key === 'Enter') confirm();
  }
</script>

<svelte:window on:keydown={handleKey} />

<div class="overlay" on:click|self={cancel}>
  <div class="dialog">
    <h3>{title}</h3>
    <p>{message}</p>
    <div class="actions">
      <button class="cancel-btn" on:click={cancel}>Cancel</button>
      <button class="confirm-btn" class:destructive on:click={confirm}>{confirmLabel}</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0; z-index: 200;
    background: rgba(0, 0, 0, 0.6);
    display: flex; align-items: center; justify-content: center;
  }
  .dialog {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 12px; padding: 24px; max-width: 400px; width: 90%;
  }
  h3 { font-size: 16px; margin-bottom: 8px; }
  p { font-size: 13px; color: var(--text-secondary); line-height: 1.5; margin-bottom: 20px; }
  .actions { display: flex; gap: 10px; justify-content: flex-end; }
  .cancel-btn {
    padding: 8px 16px; background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; color: var(--text); cursor: pointer; font-size: 13px;
  }
  .cancel-btn:hover { border-color: var(--text-secondary); }
  .confirm-btn {
    padding: 8px 16px; background: var(--accent); border: none;
    border-radius: 6px; color: white; cursor: pointer; font-size: 13px; font-weight: 500;
  }
  .confirm-btn:hover { opacity: 0.85; }
  .confirm-btn.destructive { background: var(--error); }
</style>
