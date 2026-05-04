<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { listSendQueue, cancelSend, retrySend } from '../ipc';
  import type { QueuedSendInfo } from '../ipc';
  import { showToast } from '../stores/app';

  let sends: QueuedSendInfo[] = [];
  let refreshTimer: ReturnType<typeof setInterval>;

  onMount(async () => {
    await refresh();
    refreshTimer = setInterval(refresh, 5000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });

  async function refresh() {
    try { sends = await listSendQueue(); } catch (e) { console.error(e); }
  }

  async function handleCancel(id: string) {
    try {
      await cancelSend(id);
      showToast('Send cancelled');
      await refresh();
    } catch (e: any) { showToast(`Error: ${e}`); }
  }

  async function handleRetry(id: string) {
    try {
      await retrySend(id);
      showToast('Queued for retry');
      await refresh();
    } catch (e: any) { showToast(`Error: ${e}`); }
  }

  function statusIcon(status: string): string {
    if (status === 'pending') return '⏳';
    if (status === 'in_progress') return '📡';
    if (status === 'delivered') return '✅';
    if (status.startsWith('failed')) return '❌';
    return '❓';
  }

  function statusLabel(status: string): string {
    if (status === 'pending') return 'Waiting for peer';
    if (status === 'in_progress') return 'Sending…';
    if (status === 'delivered') return 'Delivered';
    if (status.startsWith('failed:')) return status.slice(8);
    return status;
  }

  function timeAgo(ts: number): string {
    const seconds = Math.floor((Date.now() - ts) / 1000);
    if (seconds < 60) return 'just now';
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
    return `${Math.floor(seconds / 86400)}d ago`;
  }

  $: pending = sends.filter(s => s.status === 'pending' || s.status === 'in_progress');
  $: delivered = sends.filter(s => s.status === 'delivered');
  $: failed = sends.filter(s => s.status.startsWith('failed'));
</script>

<div class="queue-view">
  {#if sends.length === 0}
    <div class="empty">
      <span class="icon">📤</span>
      <p>No outbound transfers</p>
      <p class="hint">When you send files to contacts, they'll appear here</p>
    </div>
  {:else}
    {#if pending.length > 0}
      <div class="section">
        <div class="section-label">Pending ({pending.length})</div>
        {#each pending as send}
          <div class="send-card pending">
            <span class="status-icon">{statusIcon(send.status)}</span>
            <div class="info">
              <div class="filename">{send.filename}</div>
              <div class="recipient">→ {send.recipient_did.slice(0, 20)}…</div>
              <div class="meta">
                {statusLabel(send.status)} · {send.attempts} attempt{send.attempts !== 1 ? 's' : ''} · queued {timeAgo(send.queued_at)}
              </div>
            </div>
            <button class="cancel-btn" on:click={() => handleCancel(send.id)} title="Cancel">✕</button>
          </div>
        {/each}
      </div>
    {/if}

    {#if failed.length > 0}
      <div class="section">
        <div class="section-label">Failed ({failed.length})</div>
        {#each failed as send}
          <div class="send-card failed">
            <span class="status-icon">{statusIcon(send.status)}</span>
            <div class="info">
              <div class="filename">{send.filename}</div>
              <div class="recipient">→ {send.recipient_did.slice(0, 20)}…</div>
              <div class="meta">{statusLabel(send.status)}</div>
            </div>
            <div class="actions">
              <button class="retry-btn" on:click={() => handleRetry(send.id)}>Retry</button>
              <button class="cancel-btn" on:click={() => handleCancel(send.id)} title="Remove">✕</button>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    {#if delivered.length > 0}
      <div class="section">
        <div class="section-label">Delivered ({delivered.length})</div>
        {#each delivered as send}
          <div class="send-card delivered">
            <span class="status-icon">{statusIcon(send.status)}</span>
            <div class="info">
              <div class="filename">{send.filename}</div>
              <div class="recipient">→ {send.recipient_did.slice(0, 20)}…</div>
              <div class="meta">Delivered · queued {timeAgo(send.queued_at)}</div>
            </div>
            <button class="cancel-btn" on:click={() => handleCancel(send.id)} title="Remove">✕</button>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .queue-view { height: 100%; display: flex; flex-direction: column; gap: 16px; }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    flex: 1; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
  .empty p { font-size: 14px; }
  .empty .hint { font-size: 12px; }
  .section { display: flex; flex-direction: column; gap: 8px; }
  .section-label {
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-secondary); font-weight: 600;
  }
  .send-card {
    display: flex; align-items: center; gap: 12px;
    padding: 12px 14px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    transition: border-color 0.15s;
  }
  .send-card.pending { border-left: 3px solid var(--warning, #f59e0b); }
  .send-card.failed { border-left: 3px solid var(--error); }
  .send-card.delivered { border-left: 3px solid var(--success); opacity: 0.7; }
  .status-icon { font-size: 20px; flex-shrink: 0; }
  .info { flex: 1; min-width: 0; }
  .filename { font-size: 14px; font-weight: 600; }
  .recipient {
    font-size: 11px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
  }
  .meta { font-size: 11px; color: var(--text-secondary); margin-top: 2px; }
  .actions { display: flex; gap: 6px; flex-shrink: 0; }
  .cancel-btn {
    background: none; border: none; cursor: pointer;
    color: var(--text-secondary); font-size: 14px; opacity: 0.5;
    transition: opacity 0.15s;
  }
  .cancel-btn:hover { opacity: 1; }
  .retry-btn {
    padding: 4px 10px; border-radius: 4px;
    background: var(--accent); color: white;
    border: none; font-size: 11px; cursor: pointer;
  }
  .retry-btn:hover { opacity: 0.85; }
</style>
