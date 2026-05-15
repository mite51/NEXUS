<script lang="ts">
  import { onMount } from 'svelte';
  import { identity, toast as toastStore, showToast, passphrase as passStore } from './lib/stores/app';
  import { theme } from './lib/stores/theme';
  import { getIdentity, getConfig, startNode, startRelay } from './lib/ipc';
  import { initNotifications, notifyFileReceived } from './lib/notifications';
  import { listen } from '@tauri-apps/api/event';
  import Setup from './lib/views/Setup.svelte';
  import Unlock from './lib/views/Unlock.svelte';
  import Main from './lib/views/Main.svelte';
  import Toast from './lib/components/Toast.svelte';
  import ErrorBoundary from './lib/components/ErrorBoundary.svelte';

  let screen: 'loading' | 'setup' | 'unlock' | 'main' = 'loading';
  const VAULT_PATH = 'vault.json';

  onMount(async () => {
    await initNotifications();
    // Listen for file-received events from the node
    await listen<{ filename: string; from: string }>('nexus://file-received', (event) => {
      notifyFileReceived(event.payload.filename, event.payload.from);
      showToast(`\u2713 Received: ${event.payload.filename}`);
    });
  });

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

  function onSetupComplete() { screen = 'main'; autoStartIfEnabled(); }
  function onUnlockComplete() { screen = 'main'; autoStartIfEnabled(); }

  async function autoStartIfEnabled() {
    try {
      const cfg = await getConfig();
      // Start relay BEFORE node so the node can connect to it
      if (cfg.auto_start_relay) {
        const peerId = await startRelay(cfg.relay_port || 4002, cfg.relay_max_circuits || 128, 4);
        showToast(`Relay auto-started: ${peerId.slice(0, 16)}…`);
      }
      if (cfg.auto_start_node) {
        const peerId = await startNode(VAULT_PATH, $passStore);
        showToast(`Node auto-started: ${peerId.slice(0, 16)}…`);
      }
    } catch (_) {
      // Silently fail — user can start manually from Settings
    }
  }

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
<ErrorBoundary />

<style>
  :global(*) { margin: 0; padding: 0; box-sizing: border-box; }
  :global(:root), :global([data-theme="dark"]) {
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
  :global([data-theme="light"]) {
    --bg: #f5f5f5;
    --surface: #ffffff;
    --border: #e0e0e0;
    --text: #1a1a1a;
    --text-secondary: #666666;
    --accent: #4f46e5;
    --success: #16a34a;
    --warning: #d97706;
    --error: #dc2626;
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
