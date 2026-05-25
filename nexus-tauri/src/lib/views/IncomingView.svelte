<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { activePushSessions, recentPushSessions } from '../stores/push';
  import type { PushSession } from '../stores/push';
  import { listReceivedFiles, decryptReceived, removeReceived } from '../ipc';
  import type { ReceivedFileInfo } from '../ipc';
  import { showToast, passphrase } from '../stores/app';
  import { open } from '@tauri-apps/plugin-dialog';
  import ProgressBar from '../components/ProgressBar.svelte';
  import Spinner from '../components/Spinner.svelte';

  export let vaultPath: string;

  let receivedFiles: ReceivedFileInfo[] = [];
  let loading = true;
  let decryptingId: string | null = null;

  onMount(async () => {
    await loadReceived();
  });

  async function loadReceived() {
    loading = true;
    try {
      receivedFiles = await listReceivedFiles();
    } catch (e) {
      console.error('Failed to list received files:', e);
    }
    loading = false;
  }

  async function handleDecrypt(file: ReceivedFileInfo) {
    const result = await open({
      title: 'Save decrypted file to…',
      directory: true,
    });
    if (!result) return;

    decryptingId = file.id;
    try {
      const outputPath = await decryptReceived(file.id, vaultPath, $passphrase, result as string);
      showToast(`✓ Decrypted: ${outputPath}`);
      await loadReceived();
    } catch (e: any) {
      showToast(`⚠ Decrypt failed: ${e}`);
    }
    decryptingId = null;
  }

  async function handleRemove(file: ReceivedFileInfo) {
    if (!confirm(`Remove "${file.filename}" from received list?`)) return;
    try {
      await removeReceived(file.id);
      showToast(`✓ Removed ${file.filename}`);
      await loadReceived();
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  function formatTime(ts: number): string {
    return new Date(ts * 1000).toLocaleString();
  }

  function truncateDid(did: string): string {
    if (did.length > 30) return did.slice(0, 14) + '…' + did.slice(-8);
    return did;
  }

  function progressPct(s: PushSession): number {
    if (s.shards_total === 0) return 0;
    return s.shards_received / s.shards_total;
  }

  function statusBadge(status: string): { text: string; cls: string } {
    switch (status) {
      case 'accepted': return { text: 'Starting', cls: 'badge-info' };
      case 'progress': return { text: 'Receiving', cls: 'badge-info' };
      case 'complete': return { text: 'Finalizing', cls: 'badge-info' };
      case 'stored': return { text: 'Stored', cls: 'badge-success' };
      case 'denied': return { text: 'Denied', cls: 'badge-error' };
      case 'failed': return { text: 'Failed', cls: 'badge-error' };
      default: return { text: status, cls: '' };
    }
  }
</script>

<div class="incoming-view" in:fade={{ duration: 150 }}>
  <!-- Active Push Sessions -->
  {#if $activePushSessions.length > 0}
    <section class="section">
      <h3 class="section-title">
        <span class="pulse"></span>
        Active Transfers
      </h3>
      <div class="session-list">
        {#each $activePushSessions as session (session.session_id)}
          <div class="session-card active" in:fade={{ duration: 150 }}>
            <div class="card-header">
              <span class="file-icon">📥</span>
              <span class="file-name">{session.filename || 'Unknown'}</span>
              <span class="badge badge-info">
                {session.shards_received}/{session.shards_total}
              </span>
            </div>
            <ProgressBar progress={progressPct(session)} label="Receiving" />
            <div class="card-meta">
              from <span class="did">{truncateDid(session.sender_did)}</span>
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Recent Push Activity -->
  {#if $recentPushSessions.length > 0}
    <section class="section">
      <h3 class="section-title">Recent Activity</h3>
      <div class="session-list">
        {#each $recentPushSessions as session (session.session_id)}
          {@const badge = statusBadge(session.status)}
          <div class="session-card" in:fade={{ duration: 150 }}>
            <div class="card-header">
              <span class="file-icon">
                {session.status === 'stored' ? '✅' : session.status === 'denied' ? '🚫' : '❌'}
              </span>
              <span class="file-name">{session.filename || 'Unknown'}</span>
              <span class="badge {badge.cls}">{badge.text}</span>
            </div>
            <div class="card-meta">
              from <span class="did">{truncateDid(session.sender_did)}</span>
              {#if session.reason}
                <span class="reason">— {session.reason}</span>
              {/if}
            </div>
          </div>
        {/each}
      </div>
    </section>
  {/if}

  <!-- Received Files -->
  <section class="section">
    <h3 class="section-title">Received Files</h3>
    {#if loading}
      <div class="loading"><Spinner size={24} /></div>
    {:else if receivedFiles.length === 0}
      <div class="empty">
        <span class="empty-icon">📭</span>
        <p>No files received yet.</p>
        <p class="hint">Files pushed to you by contacts will appear here.</p>
      </div>
    {:else}
      <div class="file-list">
        {#each receivedFiles as file (file.id)}
          <div class="file-row">
            <div class="file-info">
              <span class="file-icon">
                {file.decrypted ? '🔓' : '🔐'}
              </span>
              <div class="file-details">
                <span class="file-name">{file.filename}</span>
                <span class="file-meta">
                  {formatBytes(file.total_size)} · {file.shard_count} shards
                  · from <span class="did">{truncateDid(file.sender_did)}</span>
                </span>
                <span class="file-date">{formatTime(file.received_at)}</span>
              </div>
            </div>
            <div class="file-actions">
              {#if !file.decrypted}
                <button
                  class="action-btn decrypt"
                  on:click={() => handleDecrypt(file)}
                  disabled={decryptingId === file.id}
                  title="Decrypt & save"
                >
                  {decryptingId === file.id ? '⏳' : '🔓'}
                </button>
              {/if}
              <button
                class="action-btn remove"
                on:click={() => handleRemove(file)}
                title="Remove from list"
              >
                🗑️
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
</div>

<style>
  .incoming-view {
    max-width: 700px;
    margin: 0 auto;
    padding: 0 24px;
  }
  .section {
    margin-bottom: 32px;
  }
  .section-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 12px;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pulse {
    width: 8px; height: 8px;
    border-radius: 50%;
    background: var(--accent);
    animation: pulse 1.5s ease-in-out infinite;
  }
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.4; }
  }
  .session-list, .file-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .session-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 16px;
  }
  .session-card.active {
    border-color: var(--accent);
    border-left: 3px solid var(--accent);
  }
  .card-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .card-meta {
    font-size: 11px;
    color: var(--text-secondary);
    margin-top: 4px;
  }
  .file-icon { font-size: 16px; flex-shrink: 0; }
  .file-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .did {
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px;
  }
  .reason {
    color: var(--error);
    font-style: italic;
  }
  .badge {
    font-size: 10px;
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 10px;
    white-space: nowrap;
  }
  .badge-info {
    background: rgba(99, 102, 241, 0.15);
    color: var(--accent);
  }
  .badge-success {
    background: rgba(34, 197, 94, 0.15);
    color: var(--success);
  }
  .badge-error {
    background: rgba(239, 68, 68, 0.15);
    color: var(--error);
  }
  .file-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px 16px;
    transition: border-color 0.15s;
  }
  .file-row:hover {
    border-color: var(--accent);
  }
  .file-info {
    display: flex;
    align-items: center;
    gap: 12px;
    flex: 1;
    min-width: 0;
  }
  .file-details {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .file-meta {
    font-size: 11px;
    color: var(--text-secondary);
  }
  .file-date {
    font-size: 10px;
    color: var(--text-secondary);
  }
  .file-actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
    margin-left: 12px;
  }
  .action-btn {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    width: 32px; height: 32px;
    display: flex; align-items: center; justify-content: center;
    cursor: pointer;
    font-size: 14px;
    transition: all 0.15s;
  }
  .action-btn:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--surface);
  }
  .action-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .action-btn.remove:hover:not(:disabled) {
    border-color: var(--error);
  }
  .loading {
    display: flex;
    justify-content: center;
    padding: 32px;
  }
  .empty {
    text-align: center;
    padding: 48px 24px;
    color: var(--text-secondary);
  }
  .empty-icon {
    font-size: 48px;
    display: block;
    margin-bottom: 12px;
  }
  .empty p { margin: 4px 0; font-size: 14px; }
  .empty .hint { font-size: 12px; }
</style>
