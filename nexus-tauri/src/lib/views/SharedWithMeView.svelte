<script lang="ts">
  import { fade } from 'svelte/transition';
  import { invoke } from '@tauri-apps/api/core';
  import { open } from '@tauri-apps/plugin-dialog';
  import { showToast, passphrase } from '../stores/app';

  export let vaultPath: string;

  let link = '';
  let downloadPath = '';
  let addToMyFiles = false;
  let pulling = false;
  let progress = '';

  async function pickFolder() {
    const result = await open({
      title: 'Choose download folder',
      directory: true,
    });
    if (result) downloadPath = result as string;
  }

  async function handlePull() {
    if (!link.trim()) {
      showToast('⚠ Paste a nexus:// link');
      return;
    }

    // Parse nexus://<peer-id>/asset/<asset-id>
    const match = link.trim().match(/^nexus:\/\/([^/]+)\/asset\/([a-f0-9]+)$/);
    if (!match) {
      showToast('⚠ Invalid link format. Expected: nexus://<peer-id>/asset/<asset-id>');
      return;
    }

    pulling = true;
    progress = 'Connecting to peer...';

    try {
      const result: any = await invoke('pull_shared_file', {
        link: link.trim(),
        vaultPath,
        passphrase: $passphrase,
        outputDir: addToMyFiles ? null : (downloadPath || null),
        addToMyFiles,
      });

      progress = '';
      showToast(`✓ Downloaded: ${result.filename} (${formatBytes(result.size)})`);
      link = '';
    } catch (e: any) {
      progress = '';
      showToast(`⚠ Pull failed: ${e}`);
    } finally {
      pulling = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
</script>

<div class="download-view" in:fade={{ duration: 150 }}>
  <div class="card">
    <div class="header">
      <span class="icon">⬇️</span>
      <h2>Download Shared File</h2>
    </div>

    <p class="description">Paste a nexus:// link to download a file shared with you.</p>

    <div class="field">
      <label for="link-input">Share Link</label>
      <input
        id="link-input"
        type="text"
        bind:value={link}
        placeholder="nexus://12D3KooW.../asset/abc123..."
        disabled={pulling}
        on:keydown={(e) => e.key === 'Enter' && handlePull()}
      />
    </div>

    <div class="field">
      <label for="download-location">Download Location</label>
      <div class="folder-row">
        <input
          id="download-location"
          type="text"
          bind:value={downloadPath}
          placeholder={addToMyFiles ? '(added to My Files)' : 'Downloads folder (default)'}
          disabled={pulling || addToMyFiles}
        />
        <button class="browse-btn" on:click={pickFolder} disabled={pulling || addToMyFiles}>
          Browse
        </button>
      </div>
    </div>

    <div class="field checkbox-field">
      <label class="checkbox-label">
        <input type="checkbox" bind:checked={addToMyFiles} disabled={pulling} />
        <span>Add to My Files</span>
        <span class="hint">(encrypt &amp; store locally instead of saving to disk)</span>
      </label>
    </div>

    {#if progress}
      <div class="progress" in:fade={{ duration: 100 }}>
        <span class="spinner">⏳</span>
        {progress}
      </div>
    {/if}

    <button
      class="pull-btn"
      on:click={handlePull}
      disabled={pulling || !link.trim()}
    >
      {#if pulling}
        Pulling...
      {:else}
        Download
      {/if}
    </button>
  </div>
</div>

<style>
  .download-view {
    height: 100%;
    display: flex;
    align-items: flex-start;
    justify-content: center;
    padding: 48px 24px;
  }
  .card {
    width: 100%;
    max-width: 520px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 32px;
  }
  .header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 8px;
  }
  .header .icon { font-size: 24px; }
  .header h2 {
    font-size: 18px;
    font-weight: 600;
    color: var(--text);
    margin: 0;
  }
  .description {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0 0 24px;
  }
  .field {
    margin-bottom: 16px;
  }
  .field label {
    display: block;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
    margin-bottom: 6px;
  }
  .field input[type="text"] {
    width: 100%;
    padding: 10px 12px;
    font-size: 13px;
    font-family: 'JetBrains Mono', monospace;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    outline: none;
    transition: border-color 0.15s;
  }
  .field input[type="text"]:focus {
    border-color: var(--accent);
  }
  .field input[type="text"]:disabled {
    opacity: 0.5;
  }
  .folder-row {
    display: flex;
    gap: 8px;
  }
  .folder-row input { flex: 1; }
  .browse-btn {
    padding: 10px 14px;
    font-size: 12px;
    background: var(--border);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    cursor: pointer;
    white-space: nowrap;
  }
  .browse-btn:hover:not(:disabled) { background: var(--text-secondary); color: var(--bg); }
  .browse-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .checkbox-field { margin-top: 4px; }
  .checkbox-label {
    display: flex !important;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text);
    cursor: pointer;
  }
  .checkbox-label input[type="checkbox"] {
    width: 16px;
    height: 16px;
    accent-color: var(--accent);
  }
  .checkbox-label .hint {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--accent);
    margin-bottom: 16px;
    padding: 8px 12px;
    background: rgba(139, 92, 246, 0.08);
    border-radius: 6px;
  }
  .spinner {
    animation: spin 1s linear infinite;
    display: inline-block;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .pull-btn {
    width: 100%;
    padding: 12px;
    font-size: 14px;
    font-weight: 600;
    background: var(--accent);
    color: white;
    border: none;
    border-radius: 8px;
    cursor: pointer;
    margin-top: 8px;
    transition: opacity 0.15s;
  }
  .pull-btn:hover:not(:disabled) { opacity: 0.85; }
  .pull-btn:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
