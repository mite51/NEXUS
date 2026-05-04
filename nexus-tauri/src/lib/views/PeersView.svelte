<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { startNode, stopNode, getNodeInfo } from '../ipc';
  import type { NodeInfo } from '../ipc';
  import { showToast, passphrase, nodeOnline } from '../stores/app';

  export let vaultPath: string;

  let info: NodeInfo = { running: false, peer_id: null, listen_addrs: [], connected_peers: [] };
  let starting = false;
  let refreshTimer: ReturnType<typeof setInterval>;

  onMount(async () => {
    await refresh();
    refreshTimer = setInterval(refresh, 3000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });

  async function refresh() {
    try {
      info = await getNodeInfo();
      nodeOnline.set(info.running);
    } catch (e) { console.error(e); }
  }

  async function handleStart() {
    starting = true;
    try {
      const peerId = await startNode(vaultPath, $passphrase);
      showToast(`Node started: ${peerId.slice(0, 16)}…`);
      await refresh();
    } catch (e: any) {
      showToast(`Start failed: ${e}`);
    }
    starting = false;
  }

  async function handleStop() {
    try {
      await stopNode();
      showToast('Node stopped');
      await refresh();
    } catch (e: any) {
      showToast(`Stop failed: ${e}`);
    }
  }

  function shortPeerId(id: string): string {
    if (id.length <= 16) return id;
    return id.slice(0, 8) + '…' + id.slice(-8);
  }
</script>

<div class="peers-view">
  <div class="node-status">
    <div class="status-row">
      <div class="indicator" class:online={info.running}></div>
      <span class="status-text">{info.running ? 'Online' : 'Offline'}</span>
      {#if info.running}
        <button class="stop-btn" on:click={handleStop}>Stop</button>
      {:else}
        <button class="start-btn" on:click={handleStart} disabled={starting}>
          {starting ? 'Starting…' : 'Start Node'}
        </button>
      {/if}
    </div>

    {#if info.running && info.peer_id}
      <div class="info-block">
        <div class="label">Peer ID</div>
        <div class="value mono">{info.peer_id}</div>
      </div>

      <div class="info-block">
        <div class="label">Listening on</div>
        {#if info.listen_addrs.length > 0}
          {#each info.listen_addrs as addr}
            <div class="value mono addr">{addr}</div>
          {/each}
        {:else}
          <div class="value muted">No listeners yet…</div>
        {/if}
      </div>
    {/if}
  </div>

  <div class="peers-section">
    <div class="section-label">Connected Peers ({info.connected_peers.length})</div>
    {#if info.connected_peers.length === 0}
      <div class="empty-peers">
        {#if info.running}
          <p>No peers connected</p>
          <p class="hint">Peers on the same network will appear via mDNS</p>
        {:else}
          <p>Start the node to discover peers</p>
        {/if}
      </div>
    {:else}
      <div class="peer-list">
        {#each info.connected_peers as peer}
          <div class="peer-card">
            <div class="peer-indicator"></div>
            <span class="peer-id">{shortPeerId(peer)}</span>
          </div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .peers-view { height: 100%; display: flex; flex-direction: column; gap: 24px; }
  .node-status {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 10px; padding: 20px;
    display: flex; flex-direction: column; gap: 16px;
  }
  .status-row {
    display: flex; align-items: center; gap: 10px;
  }
  .indicator {
    width: 10px; height: 10px; border-radius: 50%;
    background: var(--text-secondary);
    transition: background 0.2s;
  }
  .indicator.online { background: var(--success); box-shadow: 0 0 6px var(--success); }
  .status-text { font-size: 16px; font-weight: 600; flex: 1; }
  .start-btn, .stop-btn {
    padding: 6px 16px; border-radius: 6px;
    border: none; cursor: pointer; font-size: 12px;
  }
  .start-btn { background: var(--accent); color: white; }
  .start-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .stop-btn { background: var(--error); color: white; }
  .info-block { display: flex; flex-direction: column; gap: 4px; }
  .label { font-size: 11px; text-transform: uppercase; color: var(--text-secondary); letter-spacing: 0.5px; }
  .value { font-size: 13px; }
  .value.mono { font-family: 'JetBrains Mono', monospace; font-size: 11px; word-break: break-all; }
  .value.muted { color: var(--text-secondary); font-style: italic; }
  .addr { padding: 2px 0; }
  .peers-section { flex: 1; display: flex; flex-direction: column; gap: 12px; }
  .section-label {
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-secondary); font-weight: 600;
  }
  .empty-peers {
    display: flex; flex-direction: column; align-items: center;
    justify-content: center; flex: 1; gap: 4px; color: var(--text-secondary);
  }
  .empty-peers p { font-size: 14px; }
  .empty-peers .hint { font-size: 12px; }
  .peer-list { display: flex; flex-direction: column; gap: 6px; }
  .peer-card {
    display: flex; align-items: center; gap: 10px;
    padding: 10px 14px;
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 6px;
  }
  .peer-indicator {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--success);
  }
  .peer-id {
    font-family: 'JetBrains Mono', monospace;
    font-size: 12px;
  }
</style>
