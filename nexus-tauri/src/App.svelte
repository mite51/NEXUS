<script lang="ts">
  import { identity, toast as toastStore } from './lib/stores/app';
  import { getIdentity } from './lib/ipc';
  import Setup from './lib/views/Setup.svelte';
  import Unlock from './lib/views/Unlock.svelte';
  import Main from './lib/views/Main.svelte';
  import Toast from './lib/components/Toast.svelte';

  let screen: 'loading' | 'setup' | 'unlock' | 'main' = 'loading';
  const VAULT_PATH = 'vault.json';

  async function checkVault() {
    try {
      await getIdentity(VAULT_PATH, '');
      // Empty pass somehow worked (unlikely)
      screen = 'main';
    } catch (e: any) {
      if (typeof e === 'string' && e.includes('Failed to read vault')) {
        screen = 'setup';
      } else {
        screen = 'unlock';
      }
    }
  }

  function onSetupComplete() { screen = 'main'; }
  function onUnlockComplete() { screen = 'main'; }

  checkVault();
</script>

<div class="app">
  {#if screen === 'loading'}
    <div class="center-screen">
      <h1 class="logo">⚡ NEXUS</h1>
      <p class="muted">Loading...</p>
    </div>
  {:else if screen === 'setup'}
    <Setup vaultPath={VAULT_PATH} on:complete={onSetupComplete} />
  {:else if screen === 'unlock'}
    <Unlock vaultPath={VAULT_PATH} on:complete={onUnlockComplete} />
  {:else}
    <Main vaultPath={VAULT_PATH} />
  {/if}
</div>

{#if $toastStore}
  <Toast message={$toastStore} />
{/if}

<style>
  :global(*) { margin: 0; padding: 0; box-sizing: border-box; }
  :global(:root) {
    --bg: #0f0f0f;
    --surface: #1a1a1a;
    --border: #2a2a2a;
    --text: #e0e0e0;
    --text-secondary: #888888;
    --accent: #6366f1;
    --success: #22c55e;
    --warning: #f59e0b;
    --error: #ef4444;
    --radius: 8px;
  }
  :global(body) {
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, sans-serif;
    background: var(--bg);
    color: var(--text);
    height: 100vh;
    overflow: hidden;
  }
  .app { height: 100vh; display: flex; }
  .center-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
    gap: 8px;
  }
  .logo { color: var(--accent); font-size: 24px; }
  .muted { color: var(--text-secondary); font-size: 14px; }
</style>
