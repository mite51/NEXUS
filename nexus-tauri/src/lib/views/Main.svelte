<script lang="ts">
  import { onMount } from 'svelte';
  import { identity, files, currentView, nodeOnline, didShort, showToast } from '../stores/app';
  import { listFiles } from '../ipc';
  import FileGrid from '../components/FileGrid.svelte';
  import Sidebar from '../components/Sidebar.svelte';

  export let vaultPath: string;

  onMount(async () => {
    try {
      const f = await listFiles();
      files.set(f);
    } catch (e) { console.error('Failed to list files:', e); }
  });

  function handleCopyDid() {
    const id = $identity;
    if (id) {
      navigator.clipboard.writeText(id.did);
      showToast('DID copied to clipboard');
    }
  }
</script>

<Sidebar
  did={$didShort}
  view={$currentView}
  online={$nodeOnline}
  on:navigate={(e) => currentView.set(e.detail)}
  on:copyDid={handleCopyDid}
/>

<main class="main">
  <div class="toolbar">
    <h2>{
      $currentView === 'files' ? 'My Files' :
      $currentView === 'shared' ? 'Shared With Me' :
      $currentView === 'peers' ? 'Peers' : 'Store'
    }</h2>
    <div class="spacer"></div>
    {#if $currentView === 'files'}
      <button class="primary-btn">+ Encrypt File</button>
    {/if}
  </div>

  <div class="content">
    {#if $currentView === 'files'}
      <FileGrid files={$files} />
    {:else if $currentView === 'shared'}
      <div class="empty">
        <span class="icon">📨</span>
        <p>No shared files yet</p>
      </div>
    {:else if $currentView === 'peers'}
      <div class="empty">
        <span class="icon">🌐</span>
        <p>Node is offline — start it to discover peers</p>
      </div>
    {:else}
      <div class="empty">
        <span class="icon">📦</span>
        <p>Shard store stats coming soon</p>
      </div>
    {/if}
  </div>
</main>

<style>
  .main {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
  }
  .toolbar {
    padding: 12px 24px;
    border-bottom: 1px solid var(--border);
    display: flex; align-items: center; gap: 12px;
  }
  .toolbar h2 { font-size: 16px; font-weight: 600; }
  .spacer { flex: 1; }
  .primary-btn {
    background: var(--accent); color: white;
    border: none; padding: 8px 16px; border-radius: 6px;
    font-size: 13px; cursor: pointer;
  }
  .primary-btn:hover { opacity: 0.85; }
  .content { flex: 1; padding: 24px; overflow-y: auto; }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    height: 100%; gap: 12px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
  .empty p { font-size: 14px; }
</style>
