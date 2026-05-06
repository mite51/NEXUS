<script lang="ts">
  import { onMount } from 'svelte';
  import { logs, clearLogs, markLogsRead, type LogEntry } from '../stores/logs';

  let filter: 'all' | 'error' | 'warn' | 'info' | 'success' = 'all';

  $: filtered = filter === 'all' ? $logs : $logs.filter(l => l.level === filter);

  onMount(() => {
    markLogsRead();
  });

  function formatTime(ts: number): string {
    const d = new Date(ts);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  function levelIcon(level: string): string {
    switch (level) {
      case 'error': return '❌';
      case 'warn': return '⚠️';
      case 'success': return '✅';
      default: return 'ℹ️';
    }
  }
</script>

<div class="logs-view">
  <div class="logs-toolbar">
    <div class="filter-group">
      <button class:active={filter === 'all'} on:click={() => filter = 'all'}>All</button>
      <button class:active={filter === 'error'} on:click={() => filter = 'error'}>Errors</button>
      <button class:active={filter === 'warn'} on:click={() => filter = 'warn'}>Warnings</button>
      <button class:active={filter === 'info'} on:click={() => filter = 'info'}>Info</button>
    </div>
    <button class="clear-btn" on:click={clearLogs}>Clear</button>
  </div>

  <div class="logs-list">
    {#if filtered.length === 0}
      <div class="empty">No log entries</div>
    {:else}
      {#each filtered as entry (entry.id)}
        <div class="log-entry level-{entry.level}">
          <span class="log-icon">{levelIcon(entry.level)}</span>
          <span class="log-time">{formatTime(entry.timestamp)}</span>
          <span class="log-source">[{entry.source}]</span>
          <span class="log-msg">{entry.message}</span>
          {#if entry.detail}
            <div class="log-detail">{entry.detail}</div>
          {/if}
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .logs-view {
    display: flex; flex-direction: column; height: 100%;
  }
  .logs-toolbar {
    display: flex; align-items: center; gap: 12px;
    padding: 8px 0; margin-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }
  .filter-group {
    display: flex; gap: 4px;
  }
  .filter-group button {
    background: var(--surface); border: 1px solid var(--border);
    color: var(--text-secondary); padding: 4px 10px;
    border-radius: 4px; font-size: 12px; cursor: pointer;
  }
  .filter-group button.active {
    background: var(--accent); color: white; border-color: var(--accent);
  }
  .clear-btn {
    margin-left: auto;
    background: none; border: 1px solid var(--border);
    color: var(--text-secondary); padding: 4px 10px;
    border-radius: 4px; font-size: 12px; cursor: pointer;
  }
  .clear-btn:hover { border-color: var(--danger, #e11d48); color: var(--danger, #e11d48); }
  .logs-list {
    flex: 1; overflow-y: auto;
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
  }
  .empty {
    color: var(--text-secondary); text-align: center;
    padding: 48px 0; font-style: italic;
  }
  .log-entry {
    display: flex; flex-wrap: wrap; align-items: baseline; gap: 6px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    line-height: 1.4;
  }
  .log-entry:hover { background: var(--surface); }
  .log-entry.level-error { border-left: 3px solid #e11d48; }
  .log-entry.level-warn { border-left: 3px solid #f59e0b; }
  .log-entry.level-success { border-left: 3px solid var(--success); }
  .log-entry.level-info { border-left: 3px solid var(--text-secondary); }
  .log-icon { font-size: 13px; }
  .log-time { color: var(--text-secondary); flex-shrink: 0; }
  .log-source { color: var(--accent); font-weight: 500; flex-shrink: 0; }
  .log-msg { color: var(--text); flex: 1; word-break: break-word; }
  .log-detail {
    width: 100%;
    margin-top: 4px; padding: 6px 8px;
    background: var(--bg); border-radius: 4px;
    color: var(--text-secondary); font-size: 11px;
    white-space: pre-wrap; word-break: break-all;
  }
</style>
