<script lang="ts">
  import { fade, fly } from 'svelte/transition';
  import { activePushSessions } from '../stores/push';
  import type { PushSession } from '../stores/push';

  function progressPct(s: PushSession): number {
    if (s.shards_total === 0) return 0;
    return Math.round((s.shards_received / s.shards_total) * 100);
  }

  function truncateDid(did: string): string {
    if (did.length > 24) return did.slice(0, 12) + '…' + did.slice(-8);
    return did;
  }
</script>

{#if $activePushSessions.length > 0}
  <div class="push-toast-container" transition:fade={{ duration: 200 }}>
    {#each $activePushSessions as session (session.session_id)}
      <div class="push-toast" in:fly={{ y: -20, duration: 200 }} out:fade={{ duration: 150 }}>
        <div class="toast-header">
          <span class="icon">📥</span>
          <span class="filename">{session.filename || 'Unknown file'}</span>
        </div>
        <div class="toast-meta">
          from {truncateDid(session.sender_did)}
        </div>
        <div class="toast-progress">
          <div class="track">
            <div
              class="fill"
              class:complete={session.status === 'complete'}
              style="width: {progressPct(session)}%"
            ></div>
          </div>
          <span class="pct">{progressPct(session)}%</span>
        </div>
        <div class="toast-status">
          {#if session.status === 'accepted'}
            Receiving…
          {:else if session.status === 'progress'}
            {session.shards_received}/{session.shards_total} shards
          {:else if session.status === 'complete'}
            Finalizing…
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .push-toast-container {
    position: fixed;
    top: 16px;
    right: 16px;
    z-index: 999;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 320px;
  }
  .push-toast {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 12px 16px;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.3);
  }
  .toast-header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .icon { font-size: 16px; }
  .filename {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .toast-meta {
    font-size: 11px;
    color: var(--text-secondary);
    margin-bottom: 8px;
    font-family: 'JetBrains Mono', monospace;
  }
  .toast-progress {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }
  .track {
    flex: 1;
    height: 4px;
    background: var(--border);
    border-radius: 2px;
    overflow: hidden;
  }
  .fill {
    height: 100%;
    background: var(--accent);
    border-radius: 2px;
    transition: width 0.3s ease;
  }
  .fill.complete {
    background: var(--success);
  }
  .pct {
    font-size: 11px;
    color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    min-width: 32px;
    text-align: right;
  }
  .toast-status {
    font-size: 11px;
    color: var(--accent);
  }
</style>
