<script lang="ts">
  import { onMount } from 'svelte';
  import { toast } from '../stores/app';

  export let message: string;

  let visible = true;
  let fadeOut = false;

  onMount(() => {
    // Start fade-out 500ms before toast clears
    const timer = setTimeout(() => {
      fadeOut = true;
    }, 2500);
    return () => clearTimeout(timer);
  });
</script>

<div class="toast" class:fade-out={fadeOut}>
  <span class="icon">
    {#if message.startsWith('✓')}✓{:else if message.startsWith('⚠')}⚠{:else}ℹ{/if}
  </span>
  <span class="msg">{message.replace(/^[✓⚠ℹ]\s*/, '')}</span>
  <button class="close" on:click={() => toast.set(null)}>×</button>
</div>

<style>
  .toast {
    position: fixed;
    top: 16px; right: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 10px 16px;
    font-size: 13px;
    z-index: 1000;
    display: flex;
    align-items: center;
    gap: 8px;
    box-shadow: 0 4px 12px rgba(0,0,0,0.3);
    animation: slideIn 0.2s ease-out;
    transition: opacity 0.3s ease, transform 0.3s ease;
  }
  .toast.fade-out {
    opacity: 0;
    transform: translateX(20px);
  }
  .icon { font-size: 16px; }
  .msg { flex: 1; }
  .close {
    background: none; border: none; color: var(--text-secondary);
    font-size: 16px; cursor: pointer; padding: 0 4px;
  }
  .close:hover { color: var(--text); }
  @keyframes slideIn {
    from { transform: translateX(100%); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }
</style>
