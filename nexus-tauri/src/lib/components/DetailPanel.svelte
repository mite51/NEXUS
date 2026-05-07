<script lang="ts">
  import type { FileEntry } from '../ipc';
  import { createEventDispatcher } from 'svelte';
  import { formatBytes } from '../utils';

  export let file: FileEntry | null;
  const dispatch = createEventDispatcher();
</script>

{#if file}
  <aside class="detail-panel">
    <div class="detail-header">
      <h3>{file.filename}</h3>
      <button class="close-btn" on:click={() => dispatch('close')}>✕</button>
    </div>

    <div class="detail-section">
      <div class="detail-label">Status</div>
      <div class="detail-value">
        <span class="badge encrypted">Encrypted</span>
      </div>
    </div>

    <div class="detail-section">
      <div class="detail-label">Shards</div>
      <div class="detail-value">{file.shard_count}</div>
    </div>

    <div class="detail-section">
      <div class="detail-label">Size</div>
      <div class="detail-value">{formatBytes(file.total_size)}</div>
    </div>

    <div class="detail-section">
      <div class="detail-label">Owner</div>
      <div class="detail-value mono" title={file.owner}>
        {file.owner.length > 30 ? file.owner.slice(0, 20) + '...' + file.owner.slice(-8) : file.owner}
      </div>
    </div>

    <div class="detail-section">
      <div class="detail-label">Manifest</div>
      <div class="detail-value mono small">{file.manifest_path}</div>
    </div>

    <div class="detail-actions">
      <button class="action-btn primary" on:click={() => dispatch('decrypt', file)}>
        🔓 Decrypt
      </button>
      <button class="action-btn" on:click={() => dispatch('share', file)}>
        🔗 Share
      </button>
      <button class="action-btn" on:click={() => dispatch('rename', file)}>
        ✏️ Rename
      </button>
      <button class="action-btn" on:click={() => dispatch('export', file)}>
        📦 Export Bundle
      </button>
      <button class="action-btn danger" on:click={() => dispatch('delete', file)}>
        🗑 Delete
      </button>
    </div>
  </aside>
{/if}

<style>
  .detail-panel {
    width: 280px;
    background: var(--surface);
    border-left: 1px solid var(--border);
    padding: 20px;
    overflow-y: auto;
  }
  .detail-header {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: 20px;
  }
  .detail-header h3 {
    font-size: 16px; font-weight: 600;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .close-btn {
    background: none; border: none;
    color: var(--text-secondary); cursor: pointer;
    font-size: 16px; padding: 4px;
  }
  .close-btn:hover { color: var(--text); }
  .detail-section { margin-bottom: 16px; }
  .detail-label {
    font-size: 11px; color: var(--text-secondary);
    text-transform: uppercase; letter-spacing: 0.5px;
    margin-bottom: 4px;
  }
  .detail-value { font-size: 14px; }
  .detail-value.mono {
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px; word-break: break-all;
  }
  .detail-value.small { font-size: 11px; }
  .badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 4px;
    font-size: 12px;
    font-weight: 500;
  }
  .badge.encrypted {
    background: rgba(99, 102, 241, 0.15);
    color: var(--accent);
  }
  .detail-actions {
    display: flex; flex-direction: column; gap: 8px;
    margin-top: 24px;
  }
  .action-btn {
    width: 100%;
    padding: 10px 16px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
    text-align: center;
    transition: all 0.15s;
  }
  .action-btn:hover { border-color: var(--accent); }
  .action-btn.primary {
    background: var(--accent);
    border-color: var(--accent);
    color: white;
  }
  .action-btn.primary:hover { opacity: 0.85; }
  .action-btn.danger {
    border-color: var(--error); color: var(--error);
  }
  .action-btn.danger:hover {
    background: var(--error); color: white;
  }
</style>
