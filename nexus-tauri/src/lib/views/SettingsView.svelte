<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { identity, passphrase, showToast, nodeOnline } from '../stores/app';
  import { theme, toggleTheme } from '../stores/theme';
  import { addLog } from '../stores/logs';
  import { getConfig, saveConfig, getConnectivityStats, startRelay, stopRelay, getRelayInfo, startNode, stopNode, getNodeInfo } from '../ipc';
  import type { ConnectivityStats, RelayInfo, NodeInfo } from '../ipc';

  let listenPort: string = '';
  let bootstrapPeers: string = '';
  let relayServers: string = '';
  let telemetryEnabled: boolean = true;
  let autoStartNode: boolean = false;
  let autoStartRelay: boolean = false;
  let exportingKey = false;
  let saving = false;
  let connectivityStats: ConnectivityStats | null = null;
  let loadingStats = false;
  let relayInfo: RelayInfo | null = null;
  let relayPort: string = '4002';
  let relayMaxCircuits: string = '128';
  let relayStarting = false;
  let relayStopping = false;
  let nodeInfo: NodeInfo = { running: false, peer_id: null, listen_addrs: [], connected_peers: [] };
  let nodeStarting = false;
  let refreshTimer: ReturnType<typeof setInterval>;

  onMount(async () => {
    try {
      const cfg = await getConfig();
      listenPort = cfg.listen_port?.toString() ?? '';
      bootstrapPeers = cfg.bootstrap_peers.join('\n');
      relayServers = cfg.relay_servers.join('\n');
      telemetryEnabled = cfg.telemetry_enabled ?? true;
      autoStartNode = cfg.auto_start_node ?? false;
      autoStartRelay = cfg.auto_start_relay ?? false;
    } catch (_) {}
    await refreshNode();
    await refreshStats();
    await refreshRelay();
    refreshTimer = setInterval(() => { refreshNode(); refreshRelay(); }, 3000);
  });

  onDestroy(() => {
    if (refreshTimer) clearInterval(refreshTimer);
  });

  async function refreshNode() {
    try {
      nodeInfo = await getNodeInfo();
      nodeOnline.set(nodeInfo.running);
    } catch (_) {}
  }

  async function handleStartNode() {
    if (!$passphrase) { showToast('⚠ Unlock vault first'); return; }
    nodeStarting = true;
    try {
      const peerId = await startNode('vault.json', $passphrase);
      addLog('success', 'Node', `Node started`, `PeerId: ${peerId}`);
      showToast(`Node started: ${peerId.slice(0, 16)}…`);
      await refreshNode();
      await refreshStats();
    } catch (e: any) {
      let msg = String(e);
      if (msg.startsWith('Failed to start node: ')) msg = msg.slice('Failed to start node: '.length);
      let detail = msg;
      if (msg.includes('Address already in use') || msg.includes('AddrInUse') || msg.includes('address already in use') || msg.includes('10048')) {
        detail = `Port conflict — another process is already using this port. Change the listen port below.`;
      } else if (!msg || msg === 'Failed to start node') {
        detail = 'Unknown error — check Logs tab for details';
      }
      addLog('error', 'Node', `Failed to start node`, detail);
      showToast(`Start failed: ${detail}`);
    }
    nodeStarting = false;
  }

  async function handleStopNode() {
    try {
      await stopNode();
      addLog('info', 'Node', 'Node stopped');
      showToast('Node stopped');
      await refreshNode();
    } catch (e: any) {
      showToast(`Stop failed: ${e}`);
    }
  }

  async function refreshStats() {
    loadingStats = true;
    try { connectivityStats = await getConnectivityStats(); } catch (_) { connectivityStats = null; }
    loadingStats = false;
  }

  async function refreshRelay() {
    try { relayInfo = await getRelayInfo(); } catch (_) { relayInfo = null; }
  }

  async function handleStartRelay() {
    relayStarting = true;
    try {
      const peerId = await startRelay(parseInt(relayPort) || 4002, parseInt(relayMaxCircuits) || 128, 4);
      addLog('success', 'Relay', `Relay started`, `PeerId: ${peerId}`);
      showToast(`Relay started: ${peerId.slice(0, 16)}…`);
      await refreshRelay();
    } catch (e: any) {
      let msg = String(e);
      if (msg.includes('Address already in use') || msg.includes('AddrInUse') || msg.includes('10048')) {
        msg = `Port conflict — another process is using port ${relayPort}.`;
      }
      addLog('error', 'Relay', `Failed to start relay`, msg);
      showToast(`Start failed: ${msg}`);
    }
    relayStarting = false;
  }

  async function handleStopRelay() {
    relayStopping = true;
    try {
      await stopRelay();
      addLog('info', 'Relay', 'Relay stopped');
      showToast('Relay stopped');
      relayInfo = null;
    } catch (e: any) {
      showToast(`Stop failed: ${e}`);
    }
    relayStopping = false;
  }

  async function handleSave() {
    saving = true;
    try {
      await saveConfig({
        listen_port: listenPort ? parseInt(listenPort) || null : null,
        bootstrap_peers: bootstrapPeers.split('\n').map(s => s.trim()).filter(Boolean),
        relay_servers: relayServers.split('\n').map(s => s.trim()).filter(Boolean),
        telemetry_enabled: telemetryEnabled,
        auto_start_node: autoStartNode,
        auto_start_relay: autoStartRelay,
      });
      showToast('✓ Settings saved');
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
    saving = false;
  }

  async function handleExportDid() {
    const id = $identity;
    if (id) { await navigator.clipboard.writeText(id.did); showToast('✓ DID copied to clipboard'); }
  }

  async function handleExportPrePk() {
    const id = $identity;
    if (id?.pre_public_key_hex) { await navigator.clipboard.writeText(id.pre_public_key_hex); showToast('✓ PRE public key copied'); }
  }

  async function handleExportPeerId() {
    const id = $identity;
    if (id?.peer_id) { await navigator.clipboard.writeText(id.peer_id); showToast('✓ Peer ID copied'); }
  }
</script>

<div class="settings-view">
  <!-- ═══════ IDENTITY ═══════ -->
  <div class="section-card">
    <div class="section-title">Identity</div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">DID (Decentralized ID)</div>
        <div class="setting-value mono">{$identity?.did ?? 'Not loaded'}</div>
      </div>
      <button class="copy-btn" on:click={handleExportDid}>Copy</button>
    </div>

    {#if $identity?.pre_public_key_hex}
      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-label">PRE Public Key</div>
          <div class="setting-value mono truncated">{$identity.pre_public_key_hex}</div>
        </div>
        <button class="copy-btn" on:click={handleExportPrePk}>Copy</button>
      </div>
    {/if}

    {#if $identity?.peer_id}
      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-label">libp2p Peer ID</div>
          <div class="setting-value mono truncated">{$identity.peer_id}</div>
        </div>
        <button class="copy-btn" on:click={handleExportPeerId}>Copy</button>
      </div>
    {/if}
  </div>

  <!-- ═══════ NODE ═══════ -->
  <div class="section-card">
    <div class="section-title">Node</div>

    <!-- Status + Start/Stop -->
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">
          <span class="status-dot" class:online={nodeInfo.running}></span>
          {nodeInfo.running ? 'Online' : 'Offline'}
        </div>
        {#if nodeInfo.running && nodeInfo.peer_id}
          <div class="setting-value mono truncated">{nodeInfo.peer_id}</div>
        {/if}
      </div>
      {#if nodeInfo.running}
        <button class="stop-btn" on:click={handleStopNode}>⏹ Stop</button>
      {:else}
        <button class="start-btn" on:click={handleStartNode} disabled={nodeStarting}>
          {nodeStarting ? 'Starting…' : '▶ Start'}
        </button>
      {/if}
    </div>

    <!-- Running details -->
    {#if nodeInfo.running}
      <!-- Network Health (inline) -->
      {#if connectivityStats}
        <div class="health-grid">
          <div class="health-stat">
            <span class="health-value">{connectivityStats.last_nat_status}</span>
            <span class="health-label">NAT</span>
          </div>
          <div class="health-stat">
            <span class="health-value">{nodeInfo.connected_peers.length}</span>
            <span class="health-label">Peers</span>
          </div>
          <div class="health-stat">
            <span class="health-value">{connectivityStats.connections_relayed}</span>
            <span class="health-label">Relayed</span>
          </div>
          <div class="health-stat">
            <span class="health-value">{connectivityStats.hole_punch_successes}/{connectivityStats.hole_punch_attempts}</span>
            <span class="health-label">Hole Punch</span>
          </div>
          <div class="health-stat">
            <span class="health-value">{connectivityStats.relay_successes}/{connectivityStats.relay_attempts}</span>
            <span class="health-label">Relay Res.</span>
          </div>
          <div class="health-stat">
            <span class="health-value">{connectivityStats.dial_failures}</span>
            <span class="health-label">Failures</span>
          </div>
        </div>
      {/if}

      <!-- Connected Peers -->
      {#if nodeInfo.connected_peers.length > 0}
        <div class="subsection-label">Connected Peers</div>
        <div class="peers-list">
          {#each nodeInfo.connected_peers as peer}
            <div class="peer-chip">
              <span class="peer-dot"></span>
              <span class="peer-id-text">{peer.length > 20 ? peer.slice(0, 8) + '…' + peer.slice(-8) : peer}</span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="muted" style="padding: 8px 0; font-size: 11px;">No peers connected yet.</div>
      {/if}

      <!-- Listen Addrs -->
      {#if nodeInfo.listen_addrs.length > 0}
        <div class="subsection-label">Listening On</div>
        {#each nodeInfo.listen_addrs as addr}
          <div class="setting-value mono" style="font-size: 10px; padding: 1px 0;">{addr}</div>
        {/each}
      {/if}
    {/if}

    <!-- Config -->
    <div class="config-divider"></div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Auto-start</div>
        <div class="setting-desc">Start node when app opens</div>
      </div>
      <label class="toggle-label">
        <input type="checkbox" bind:checked={autoStartNode} />
        <span class="toggle-text">{autoStartNode ? 'On' : 'Off'}</span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Listen Port</div>
        <div class="setting-desc">Leave empty for random</div>
      </div>
      <input type="text" class="setting-input" placeholder="0" bind:value={listenPort} />
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Bootstrap Peers</div>
        <div class="setting-desc">Connect to on start (one per line)</div>
      </div>
    </div>
    <textarea class="multi-input" placeholder="/ip4/1.2.3.4/tcp/4001/p2p/12D3Koo..." bind:value={bootstrapPeers}></textarea>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Relay Servers</div>
        <div class="setting-desc">For NAT traversal (one per line)</div>
      </div>
    </div>
    <textarea class="multi-input" placeholder="/ip4/1.2.3.4/tcp/4001/p2p/12D3Koo..." bind:value={relayServers}></textarea>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Telemetry</div>
        <div class="setting-desc">Log connectivity events</div>
      </div>
      <label class="toggle-label">
        <input type="checkbox" bind:checked={telemetryEnabled} />
        <span class="toggle-text">{telemetryEnabled ? 'On' : 'Off'}</span>
      </label>
    </div>

    <div class="save-row">
      <button class="save-btn" on:click={handleSave} disabled={saving}>
        {saving ? 'Saving…' : 'Save Settings'}
      </button>
      <span class="save-hint">Takes effect on next node restart</span>
    </div>
  </div>

  <!-- ═══════ RELAY ═══════ -->
  <div class="section-card">
    <div class="section-title">Relay</div>

    <!-- Status + Start/Stop -->
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">
          <span class="status-dot" class:online={relayInfo?.running}></span>
          {relayInfo?.running ? 'Running' : 'Stopped'}
        </div>
        {#if relayInfo?.running && relayInfo?.peer_id}
          <div class="setting-value mono truncated">{relayInfo.peer_id}</div>
        {/if}
      </div>
      {#if relayInfo?.running}
        <button class="stop-btn" on:click={handleStopRelay} disabled={relayStopping}>
          {relayStopping ? 'Stopping…' : '⏹ Stop'}
        </button>
      {:else}
        <button class="start-btn" on:click={handleStartRelay} disabled={relayStarting}>
          {relayStarting ? 'Starting…' : '▶ Start'}
        </button>
      {/if}
    </div>

    <!-- Running details -->
    {#if relayInfo?.running}
      <div class="health-grid">
        <div class="health-stat">
          <span class="health-value">{relayInfo.stats.connected_peers}</span>
          <span class="health-label">Peers</span>
        </div>
        <div class="health-stat">
          <span class="health-value">{relayInfo.stats.active_reservations}</span>
          <span class="health-label">Reservations</span>
        </div>
        <div class="health-stat">
          <span class="health-value">{relayInfo.stats.total_circuits}</span>
          <span class="health-label">Circuits</span>
        </div>
      </div>

      {#if relayInfo.stats.public_ip}
        <div class="setting-row">
          <div class="setting-info">
            <div class="setting-label">Public IP</div>
            <div class="setting-value mono">{relayInfo.stats.public_ip}</div>
          </div>
        </div>
      {/if}

      {#if relayInfo.stats.listen_addrs.length > 0}
        <div class="subsection-label">Listening On</div>
        {#each relayInfo.stats.listen_addrs as addr}
          <div class="setting-value mono" style="font-size: 10px; padding: 1px 0;">{addr}/p2p/{relayInfo.peer_id}</div>
        {/each}
      {/if}
    {/if}

    <!-- Config -->
    <div class="config-divider"></div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Auto-start</div>
        <div class="setting-desc">Start relay when app opens</div>
      </div>
      <label class="toggle-label">
        <input type="checkbox" bind:checked={autoStartRelay} />
        <span class="toggle-text">{autoStartRelay ? 'On' : 'Off'}</span>
      </label>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Port</div>
        <div class="setting-desc">TCP + QUIC listen port</div>
      </div>
      <input type="text" class="setting-input" placeholder="4002" bind:value={relayPort} />
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Max Circuits</div>
        <div class="setting-desc">Concurrent relayed connections</div>
      </div>
      <input type="text" class="setting-input" placeholder="128" bind:value={relayMaxCircuits} />
    </div>
  </div>

  <!-- ═══════ STORAGE ═══════ -->
  <div class="section-card">
    <div class="section-title">Storage</div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Shard Store</div>
        <div class="setting-value mono">.nexus-store/</div>
      </div>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Vault File</div>
        <div class="setting-value mono">vault.json</div>
      </div>
    </div>
  </div>

  <!-- ═══════ APPEARANCE ═══════ -->
  <div class="section-card">
    <div class="section-title">Appearance</div>
    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Theme</div>
        <div class="setting-desc">Switch between dark and light mode</div>
      </div>
      <button class="theme-toggle" on:click={toggleTheme}>
        {$theme === 'dark' ? '☀️ Light' : '🌙 Dark'}
      </button>
    </div>
  </div>

  <!-- ═══════ ABOUT ═══════ -->
  <div class="section-card">
    <div class="section-title">About</div>
    <div class="about-info">
      <p><strong>NEXUS</strong> v0.1.0</p>
      <p class="muted">Zero-knowledge file sharing with proxy re-encryption</p>
      <p class="muted small">Ed25519 + secp256k1 · libp2p · Umbral PRE · Content-addressed storage</p>
    </div>
  </div>
</div>

<style>
  .settings-view {
    display: flex; flex-direction: column; gap: 20px;
    max-width: 720px;
  }
  .section-card {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 10px; padding: 20px;
  }
  .section-title {
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-secondary); font-weight: 600; margin-bottom: 16px;
  }
  .setting-row {
    display: flex; align-items: center; justify-content: space-between;
    padding: 10px 0; border-bottom: 1px solid var(--border);
  }
  .setting-row:last-child { border-bottom: none; }
  .setting-info { flex: 1; min-width: 0; }
  .setting-label { font-size: 13px; font-weight: 500; display: flex; align-items: center; gap: 4px; }
  .setting-desc { font-size: 11px; color: var(--text-secondary); margin-top: 2px; }
  .setting-value { font-size: 12px; color: var(--text-secondary); margin-top: 2px; }
  .setting-value.mono, .truncated {
    font-family: 'JetBrains Mono', monospace; font-size: 11px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 400px;
  }
  .subsection-label {
    font-size: 11px; font-weight: 600; color: var(--text-secondary);
    text-transform: uppercase; letter-spacing: 0.3px;
    padding: 12px 0 4px;
  }
  .config-divider {
    border-top: 1px solid var(--border); margin: 16px 0 8px;
  }
  .copy-btn {
    padding: 4px 12px; background: var(--bg); border: 1px solid var(--border);
    border-radius: 4px; color: var(--text); font-size: 11px; cursor: pointer;
    flex-shrink: 0;
  }
  .copy-btn:hover { border-color: var(--accent); }
  .theme-toggle {
    padding: 6px 14px; background: var(--accent); border: none;
    border-radius: 6px; color: white; font-size: 12px;
    cursor: pointer; font-weight: 500;
  }
  .theme-toggle:hover { opacity: 0.85; }
  .setting-input {
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 4px; color: var(--text); padding: 6px 10px;
    font-size: 12px; width: 80px; outline: none;
    font-family: 'JetBrains Mono', monospace;
  }
  .setting-input:focus { border-color: var(--accent); }
  .multi-input {
    width: 100%; min-height: 56px; padding: 8px 10px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 4px; color: var(--text); font-size: 11px;
    font-family: 'JetBrains Mono', monospace;
    resize: vertical; outline: none; margin-top: 6px;
  }
  .multi-input:focus { border-color: var(--accent); }
  .save-row {
    display: flex; align-items: center; gap: 12px; margin-top: 14px;
  }
  .start-btn, .save-btn {
    padding: 8px 16px; background: var(--accent); border: none;
    border-radius: 6px; color: white; font-size: 12px;
    cursor: pointer; font-weight: 500;
  }
  .start-btn:hover, .save-btn:hover { opacity: 0.85; }
  .start-btn:disabled, .save-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .save-hint { font-size: 11px; color: var(--text-secondary); }
  .stop-btn {
    padding: 8px 16px; background: var(--error); border: none;
    border-radius: 6px; color: white; font-size: 12px;
    cursor: pointer; font-weight: 500;
  }
  .stop-btn:hover { opacity: 0.85; }
  .stop-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .status-dot {
    display: inline-block; width: 8px; height: 8px;
    border-radius: 50%; background: var(--text-secondary);
    margin-right: 4px;
  }
  .status-dot.online { background: var(--success); }
  .about-info p { font-size: 13px; line-height: 1.6; }
  .about-info .muted { color: var(--text-secondary); }
  .about-info .small { font-size: 11px; }
  .health-grid {
    display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px;
    margin: 12px 0;
  }
  .health-stat {
    display: flex; flex-direction: column; align-items: center;
    background: var(--bg); border-radius: 8px; padding: 10px 6px;
  }
  .health-value {
    font-size: 16px; font-weight: 600; color: var(--text);
    font-family: 'JetBrains Mono', monospace;
  }
  .health-label {
    font-size: 9px; text-transform: uppercase; letter-spacing: 0.3px;
    color: var(--text-secondary); margin-top: 3px;
  }
  .muted { color: var(--text-secondary); font-size: 12px; }
  .toggle-label {
    display: flex; align-items: center; gap: 8px; cursor: pointer;
  }
  .toggle-label input[type="checkbox"] {
    width: 16px; height: 16px; accent-color: var(--accent);
  }
  .toggle-text { font-size: 12px; color: var(--text-secondary); }
  .peers-list {
    display: flex; flex-wrap: wrap; gap: 6px;
    padding: 4px 0 8px;
  }
  .peer-chip {
    display: flex; align-items: center; gap: 6px;
    padding: 4px 10px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 4px;
  }
  .peer-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: var(--success);
  }
  .peer-id-text {
    font-family: 'JetBrains Mono', monospace;
    font-size: 10px; color: var(--text-secondary);
  }
</style>
