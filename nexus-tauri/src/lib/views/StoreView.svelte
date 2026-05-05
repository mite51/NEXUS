<script lang="ts">
  import { onMount } from 'svelte';
  import { getStoreStats, listShards, verifyStore } from '../ipc';
  import type { StoreStatsResult, ShardInfo, VerifyResult } from '../ipc';
  import { showToast } from '../stores/app';

  let stats: StoreStatsResult | null = null;
  let shards: ShardInfo[] = [];
  let verifyResult: VerifyResult | null = null;
  let verifying = false;
  let showShards = false;
  let error = '';

  onMount(async () => {
    try {
      stats = await getStoreStats();
    } catch (e: any) {
      error = typeof e === 'string' ? e : 'Failed to load store stats';
    }
  });

  async function handleLoadShards() {
    try {
      shards = await listShards();
      showShards = true;
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
  }

  async function handleVerify() {
    verifying = true;
    try {
      verifyResult = await verifyStore();
      if (verifyResult.corrupted.length === 0) {
        showToast(`✓ All ${verifyResult.valid} shards verified`);
      } else {
        showToast(`⚠ ${verifyResult.corrupted.length} corrupted shards found`);
      }
    } catch (e: any) {
      showToast(`⚠ Verify failed: ${e}`);
    }
    verifying = false;
  }

  async function copyCid(cid: string) {
    await navigator.clipboard.writeText(cid);
    showToast('✓ CID copied');
  }

  function formatBytes(bytes: number): string {
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / 1048576).toFixed(2)} MB`;
  }
</script>

{#if error}
  <div class="empty">
    <span class="icon">⚠️</span>
    <p>{error}</p>
  </div>
{:else if stats}
  <div class="store-panel">
    <div class="stat-card">
      <div class="stat-icon">📦</div>
      <div class="stat-content">
        <div class="stat-value">{stats.shard_count}</div>
        <div class="stat-label">Shards stored</div>
      </div>
    </div>

    <div class="stat-card">
      <div class="stat-icon">💾</div>
      <div class="stat-content">
        <div class="stat-value">{formatBytes(stats.total_bytes)}</div>
        <div class="stat-label">Total size</div>
      </div>
    </div>

    <div class="stat-card">
      <div class="stat-icon">{verifyResult ? (verifyResult.corrupted.length === 0 ? '✓' : '⚠') : '?'}</div>
      <div class="stat-content">
        <div class="stat-value" class:success={verifyResult?.corrupted.length === 0} class:danger={verifyResult && verifyResult.corrupted.length > 0}>
          {verifyResult ? (verifyResult.corrupted.length === 0 ? 'Healthy' : `${verifyResult.corrupted.length} corrupted`) : 'Unverified'}
        </div>
        <div class="stat-label">Integrity status</div>
      </div>
    </div>
  </div>

  <div class="actions">
    <button class="action-btn" on:click={handleVerify} disabled={verifying}>
      {verifying ? 'Verifying…' : '🔍 Verify All Shards'}
    </button>
    <button class="action-btn" on:click={handleLoadShards}>
      {showShards ? '↻ Refresh List' : '📋 Show Shard List'}
    </button>
  </div>

  {#if showShards && shards.length > 0}
    <div class="shard-list">
      <div class="shard-header">
        <span class="col-cid">CID</span>
        <span class="col-size">Size</span>
        <span class="col-action"></span>
      </div>
      {#each shards as shard}
        <div class="shard-row" class:corrupted={verifyResult?.corrupted.includes(shard.cid)}>
          <span class="col-cid mono" title={shard.cid}>
            {shard.cid.slice(0, 12)}…{shard.cid.slice(-8)}
          </span>
          <span class="col-size">{formatBytes(shard.size)}</span>
          <button class="copy-btn" on:click={() => copyCid(shard.cid)} title="Copy full CID">
            📋
          </button>
        </div>
      {/each}
    </div>
  {:else if showShards}
    <p class="muted">No shards in store.</p>
  {/if}
{:else}
  <div class="empty">
    <span class="icon">📦</span>
    <p>Loading store info...</p>
  </div>
{/if}

<style>
  .store-panel {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 16px;
    margin-bottom: 24px;
  }
  .stat-card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 20px;
    display: flex; align-items: center; gap: 16px;
  }
  .stat-icon { font-size: 32px; }
  .stat-value { font-size: 24px; font-weight: 600; }
  .stat-value.success { color: var(--success); }
  .stat-value.danger { color: var(--danger, #ef4444); }
  .stat-label { font-size: 12px; color: var(--text-secondary); margin-top: 2px; }
  .actions { display: flex; gap: 8px; margin-bottom: 20px; }
  .action-btn {
    background: var(--surface); color: var(--text);
    border: 1px solid var(--border); padding: 8px 16px;
    border-radius: 6px; font-size: 13px; cursor: pointer;
  }
  .action-btn:hover { border-color: var(--accent); }
  .action-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .shard-list {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: var(--radius); overflow: hidden;
  }
  .shard-header {
    display: grid; grid-template-columns: 1fr 100px 40px;
    padding: 8px 12px; font-size: 11px; text-transform: uppercase;
    letter-spacing: 0.3px; color: var(--text-secondary);
    border-bottom: 1px solid var(--border); font-weight: 600;
  }
  .shard-row {
    display: grid; grid-template-columns: 1fr 100px 40px;
    padding: 8px 12px; border-bottom: 1px solid var(--border);
    align-items: center; transition: background 0.1s;
  }
  .shard-row:last-child { border-bottom: none; }
  .shard-row:hover { background: var(--bg); }
  .shard-row.corrupted { background: rgba(239, 68, 68, 0.1); }
  .col-cid { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .col-size { font-size: 12px; color: var(--text-secondary); }
  .mono { font-family: 'JetBrains Mono', monospace; font-size: 12px; }
  .copy-btn {
    background: none; border: none; cursor: pointer;
    font-size: 14px; padding: 2px; border-radius: 4px;
    opacity: 0.6; transition: opacity 0.1s;
  }
  .copy-btn:hover { opacity: 1; }
  .muted { color: var(--text-secondary); font-size: 13px; margin-top: 12px; }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    height: 100%; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
</style>
