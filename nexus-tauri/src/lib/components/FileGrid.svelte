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
  <div class="grid">
    {#each files as file, i (file.manifest_path)}
      <button
        class="card"
        type="button"
        on:click={() => dispatch('select', file)}
        in:fly={{ y: 20, duration: 200, delay: Math.min(i * 30, 300) }}
      >
        <div class="card-icon">{fileIcon(file.filename)}</div>
        <div class="card-name" title={file.filename}>{file.filename}</div>
        <div class="card-meta">
          {file.shard_count} shard{file.shard_count !== 1 ? 's' : ''}
          {#if file.total_size}• {formatBytes(file.total_size)}{/if}
        </div>
      </button>
    {/each}
  </div>
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: 16px;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 16px;
    cursor: pointer;
    transition: all 0.15s;
    text-align: left;
    font: inherit;
    color: var(--text);
    width: 100%;
  }
  .card:hover {
    border-color: var(--accent);
    transform: translateY(-2px);
  }
  .card-icon { font-size: 28px; margin-bottom: 8px; }
  .card-name {
    font-size: 14px; font-weight: 500; margin-bottom: 4px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .card-meta { font-size: 12px; color: var(--text-secondary); }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    height: 100%; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
  .empty p { font-size: 14px; }
  .empty .hint { font-size: 12px; }
</style>
