<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { fly, fade } from 'svelte/transition';
  import type { FileEntry } from '../ipc';
  import { fileIcon, formatBytes } from '../utils';

  export let files: FileEntry[];
  const dispatch = createEventDispatcher();
</script>

{#if files.length === 0}
  <div class="empty" in:fade={{ duration: 200 }}>
    <span class="icon">📁</span>
    <p>No encrypted files yet</p>
    <p class="hint">Drop a file here or click "Encrypt File" to get started</p>
  </div>
{:else}
  <div class="list-view">
    <div class="list-header">
      <span class="col-icon"></span>
      <span class="col-name">Name</span>
      <span class="col-size">Size</span>
      <span class="col-shards">Shards</span>
      <span class="col-owner">Owner</span>
    </div>
    {#each files as file, i (file.manifest_path)}
      <button
        class="list-row"
        type="button"
        on:click={() => dispatch('select', file)}
        in:fly={{ y: 10, duration: 150, delay: Math.min(i * 20, 200) }}
      >
        <span class="col-icon">{fileIcon(file.filename)}</span>
        <span class="col-name" title={file.filename}>{file.filename}</span>
        <span class="col-size">{file.total_size ? formatBytes(file.total_size) : '—'}</span>
        <span class="col-shards">{file.shard_count}</span>
        <span class="col-owner" title={file.owner}>{file.owner.length > 20 ? file.owner.slice(0, 16) + '…' : file.owner}</span>
      </button>
    {/each}
  </div>
{/if}

<style>
  .list-view {
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  .list-header {
    display: flex;
    align-items: center;
    padding: 8px 12px;
    border-bottom: 1px solid var(--border);
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-secondary);
    position: sticky;
    top: 0;
    background: var(--bg);
    z-index: 1;
  }
  .list-row {
    display: flex;
    align-items: center;
    padding: 8px 12px;
    border: none;
    border-bottom: 1px solid var(--border);
    background: none;
    cursor: pointer;
    transition: background 0.1s;
    text-align: left;
    font: inherit;
    color: var(--text);
    width: 100%;
  }
  .list-row:hover {
    background: var(--surface);
  }
  .list-row:last-child {
    border-bottom: none;
  }
  .col-icon { width: 32px; font-size: 16px; flex-shrink: 0; }
  .col-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }
  .col-size {
    width: 80px;
    text-align: right;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .col-shards {
    width: 60px;
    text-align: center;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .col-owner {
    width: 140px;
    text-align: right;
    color: var(--text-secondary);
    font-size: 11px;
    flex-shrink: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    height: 100%; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
  .empty p { font-size: 14px; }
  .empty .hint { font-size: 12px; }
</style>
