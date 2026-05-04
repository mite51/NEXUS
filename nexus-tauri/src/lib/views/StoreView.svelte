<script lang="ts">
  import { onMount } from 'svelte';
  import { getStoreStats } from '../ipc';
  import type { StoreStatsResult } from '../ipc';

  let stats: StoreStatsResult | null = null;
  let error = '';

  onMount(async () => {
    try {
      stats = await getStoreStats();
    } catch (e: any) {
      error = typeof e === 'string' ? e : 'Failed to load store stats';
    }
  });

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
      <div class="stat-icon">✓</div>
      <div class="stat-content">
        <div class="stat-value success">Healthy</div>
        <div class="stat-label">Integrity status</div>
      </div>
    </div>
  </div>

  <div class="actions">
    <button class="action-btn">Verify All Shards</button>
  </div>
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
  .stat-label { font-size: 12px; color: var(--text-secondary); margin-top: 2px; }
  .actions { margin-top: 8px; }
  .action-btn {
    background: var(--surface); color: var(--text);
    border: 1px solid var(--border); padding: 8px 16px;
    border-radius: 6px; font-size: 13px; cursor: pointer;
  }
  .action-btn:hover { border-color: var(--accent); }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    height: 100%; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
</style>
