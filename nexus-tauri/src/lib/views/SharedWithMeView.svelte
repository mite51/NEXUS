<script lang="ts">
  import { onMount } from 'svelte';
  import { listReceivedFiles, decryptReceived, removeReceived, pickSaveLocation } from '../ipc';
  import type { ReceivedFileInfo } from '../ipc';
  import { showToast, passphrase } from '../stores/app';

  export let vaultPath: string;

  let files: ReceivedFileInfo[] = [];

  onMount(async () => {
    await refresh();
  });

  async function refresh() {
    try { files = await listReceivedFiles(); } catch (e) { console.error(e); }
  }

  async function handleDecrypt(file: ReceivedFileInfo) {
    const savePath = await pickSaveLocation(file.filename);
    if (!savePath) return;

    try {
      const out = await decryptReceived(file.id, vaultPath, $passphrase, savePath);
      showToast(`✓ Decrypted → ${out}`);
      await refresh();
    } catch (e: any) {
      showToast(`Decrypt error: ${e}`);
    }
  }

  async function handleRemove(id: string) {
    try {
      await removeReceived(id);
      showToast('Removed');
      await refresh();
    } catch (e: any) {
      showToast(`Error: ${e}`);
    }
  }

  function timeAgo(ts: number): string {
    const seconds = Math.floor((Date.now() - ts) / 1000);
    if (seconds < 60) return 'just now';
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
    return `${Math.floor(seconds / 86400)}d ago`;
  }
</script>

<div class="shared-view">
  {#if files.length === 0}
    <div class="empty">
      <span class="icon">📨</span>
      <p>No shared files yet</p>
      <p class="hint">When peers send or share files with you, they'll appear here</p>
    </div>
  {:else}
    <div class="file-list">
      {#each files as file}
        <div class="file-card" class:decrypted={file.decrypted}>
          <div class="file-icon">
            {#if file.has_share_grant}🔑{:else}📄{/if}
          </div>
          <div class="info">
            <div class="filename">{file.filename}</div>
            <div class="sender">From: {file.sender_did.slice(0, 20)}…</div>
            <div class="meta">
              {#if file.has_share_grant}
                <span class="badge pre">PRE Shared</span>
              {:else}
                <span class="badge direct">Direct</span>
              {/if}
              · {timeAgo(file.received_at)}
              {#if file.decrypted}
                · <span class="decrypted-badge">Decrypted ✓</span>
              {/if}
            </div>
          </div>
          <div class="actions">
            {#if !file.decrypted}
              <button class="decrypt-btn" on:click={() => handleDecrypt(file)}>
                Decrypt
              </button>
            {:else}
              <button class="decrypt-btn secondary" on:click={() => handleDecrypt(file)}>
                Re-decrypt
              </button>
            {/if}
            <button class="remove-btn" on:click={() => handleRemove(file.id)} title="Remove">✕</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .shared-view { height: 100%; display: flex; flex-direction: column; }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    flex: 1; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
  .empty p { font-size: 14px; }
  .empty .hint { font-size: 12px; }
  .file-list {
    display: flex; flex-direction: column; gap: 8px;
    flex: 1; overflow-y: auto;
  }
  .file-card {
    display: flex; align-items: center; gap: 12px;
    padding: 14px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    transition: border-color 0.15s;
  }
  .file-card:hover { border-color: var(--accent); }
  .file-card.decrypted { opacity: 0.7; }
  .file-icon { font-size: 24px; flex-shrink: 0; }
  .info { flex: 1; min-width: 0; }
  .filename { font-size: 14px; font-weight: 600; }
  .sender {
    font-size: 11px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
  }
  .meta { font-size: 11px; color: var(--text-secondary); margin-top: 4px; display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
  .badge {
    display: inline-block; font-size: 10px;
    padding: 1px 6px; border-radius: 3px;
  }
  .badge.pre { background: rgba(139, 92, 246, 0.15); color: #8b5cf6; }
  .badge.direct { background: rgba(59, 130, 246, 0.15); color: #3b82f6; }
  .decrypted-badge { color: var(--success); }
  .actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
  .decrypt-btn {
    padding: 6px 14px; border-radius: 6px;
    background: var(--accent); color: white;
    border: none; font-size: 12px; cursor: pointer;
  }
  .decrypt-btn:hover { opacity: 0.85; }
  .decrypt-btn.secondary {
    background: transparent; color: var(--accent);
    border: 1px solid var(--accent);
  }
  .remove-btn {
    background: none; border: none; cursor: pointer;
    color: var(--text-secondary); font-size: 14px; opacity: 0.5;
    transition: opacity 0.15s;
  }
  .remove-btn:hover { opacity: 1; }
</style>
