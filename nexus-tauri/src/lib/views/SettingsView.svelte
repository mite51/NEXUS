<script lang="ts">
  import { onMount } from 'svelte';
  import { identity, passphrase, showToast, nodeOnline } from '../stores/app';
  import { theme, toggleTheme } from '../stores/theme';
  import { getConfig, saveConfig, getConnectivityStats, startRelay, stopRelay, getRelayInfo } from '../ipc';
  import type { ConnectivityStats, RelayInfo } from '../ipc';

  let listenPort: string = '';
  let bootstrapPeers: string = '';
  let relayServers: string = '';
  let telemetryEnabled: boolean = true;
  let exportingKey = false;
  let saving = false;
  let connectivityStats: ConnectivityStats | null = null;
  let loadingStats = false;
  let relayInfo: RelayInfo | null = null;
  let relayPort: string = '4002';
  let relayMaxCircuits: string = '128';
  let relayStarting = false;
  let relayStopping = false;

  onMount(async () => {
    try {
      const cfg = await getConfig();
      listenPort = cfg.listen_port?.toString() ?? '';
      bootstrapPeers = cfg.bootstrap_peers.join('\n');
      relayServers = cfg.relay_servers.join('\n');
      telemetryEnabled = cfg.telemetry_enabled ?? true;
    } catch (_) {}
    await refreshStats();
    await refreshRelay();
  });

  async function refreshStats() {
    loadingStats = true;
    try {
      connectivityStats = await getConnectivityStats();
    } catch (_) {
      connectivityStats = null;
    }
    loadingStats = false;
  }

  async function refreshRelay() {
    try {
      relayInfo = await getRelayInfo();
    } catch (_) {
      relayInfo = null;
    }
  }

  async function handleStartRelay() {
    if (!$passphrase) {
      showToast('⚠ Unlock vault first');
      return;
    }
    relayStarting = true;
    try {
      const peerId = await startRelay(
        'vault.json',
        $passphrase,
        parseInt(relayPort) || 4002,
        parseInt(relayMaxCircuits) || 128,
        4
      );
      showToast(`✓ Relay started: ${peerId.slice(0, 16)}...`);
      await refreshRelay();
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
    relayStarting = false;
  }

  async function handleStopRelay() {
    relayStopping = true;
    try {
      await stopRelay();
      showToast('✓ Relay stopped');
      relayInfo = null;
    } catch (e: any) {
      showToast(`⚠ ${e}`);
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
      });
      showToast('✓ Settings saved');
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
    saving = false;
  }

  async function handleExportDid() {
    const id = $identity;
    if (id) {
      await navigator.clipboard.writeText(id.did);
      showToast('✓ DID copied to clipboard');
    }
  }

  async function handleExportPrePk() {
    const id = $identity;
    if (id?.pre_public_key_hex) {
      await navigator.clipboard.writeText(id.pre_public_key_hex);
      showToast('✓ PRE public key copied');
    }
  }

  async function handleExportPeerId() {
    const id = $identity;
    if (id?.peer_id) {
      await navigator.clipboard.writeText(id.peer_id);
      showToast('✓ Peer ID copied');
    }
  }
</script>

<div class="settings-view">
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

  <div class="section-card">
    <div class="section-title">Network</div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Node Status</div>
        <div class="setting-value">
          <span class="status-dot" class:online={$nodeOnline}></span>
          {$nodeOnline ? 'Online' : 'Offline'}
        </div>
      </div>
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Listen Port</div>
        <div class="setting-desc">Leave empty for random port (default)</div>
      </div>
      <input type="text" class="setting-input" placeholder="0" bind:value={listenPort} />
    </div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Bootstrap Peers</div>
        <div class="setting-desc">Multiaddrs to connect to on start (one per line)</div>
      </div>
    </div>
    <textarea class="bootstrap-input" placeholder="/ip4/1.2.3.4/tcp/4001/p2p/12D3Koo..."
              bind:value={bootstrapPeers}></textarea>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Relay Servers</div>
        <div class="setting-desc">Multiaddrs of relay nodes for NAT traversal (one per line)</div>
      </div>
    </div>
    <textarea class="bootstrap-input" placeholder="/ip4/1.2.3.4/tcp/4001/p2p/12D3Koo..."
              bind:value={relayServers}></textarea>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Connectivity Telemetry</div>
        <div class="setting-desc">Log connection events for diagnostics</div>
      </div>
      <label class="toggle-label">
        <input type="checkbox" bind:checked={telemetryEnabled} />
        <span class="toggle-text">{telemetryEnabled ? 'On' : 'Off'}</span>
      </label>
    </div>

    <div class="save-row">
      <button class="save-btn" on:click={handleSave} disabled={saving}>
        {saving ? 'Saving…' : 'Save Network Settings'}
      </button>
      <span class="save-hint">Takes effect on next node restart</span>
    </div>
  </div>

  <div class="section-card">
    <div class="section-title">Relay Server</div>

    <div class="setting-row">
      <div class="setting-info">
        <div class="setting-label">Status</div>
        <div class="setting-value">
          <span class="status-dot" class:online={relayInfo?.running}></span>
          {relayInfo?.running ? 'Running' : 'Stopped'}
        </div>
      </div>
    </div>

    {#if relayInfo?.running}
      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-label">Relay Peer ID</div>
          <div class="setting-value mono truncated">{relayInfo.peer_id}</div>
        </div>
      </div>

      <div class="health-grid">
        <div class="health-stat">
          <span class="health-value">{relayInfo.stats.connected_peers}</span>
          <span class="health-label">Connected</span>
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

      {#if relayInfo.stats.listen_addrs.length > 0}
        <div class="setting-row">
          <div class="setting-info">
            <div class="setting-label">Listen Addresses</div>
            {#each relayInfo.stats.listen_addrs as addr}
              <div class="setting-value mono" style="font-size: 10px;">{addr}/p2p/{relayInfo.peer_id}</div>
            {/each}
          </div>
        </div>
      {/if}

      <div class="save-row">
        <button class="stop-btn" on:click={handleStopRelay} disabled={relayStopping}>
          {relayStopping ? 'Stopping…' : '⏹ Stop Relay'}
        </button>
        <button class="refresh-btn" on:click={refreshRelay}>↻ Refresh</button>
      </div>
    {:else}
      <div class="setting-row">
        <div class="setting-info">
          <div class="setting-label">Port</div>
          <div class="setting-desc">TCP and QUIC listen port for relay</div>
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

      <div class="save-row">
        <button class="save-btn" on:click={handleStartRelay} disabled={relayStarting}>
          {relayStarting ? 'Starting…' : '▶ Start Relay'}
        </button>
        <span class="save-hint">Run a relay to help NATted peers connect</span>
      </div>
    {/if}
  </div>

  <div class="section-card">
    <div class="section-title">Network Health</div>

    {#if connectivityStats}
      <div class="health-grid">
        <div class="health-stat">
          <span class="health-value">{connectivityStats.last_nat_status}</span>
          <span class="health-label">NAT Status</span>
        </div>
        <div class="health-stat">
          <span class="health-value">{connectivityStats.connections_total}</span>
          <span class="health-label">Connections</span>
        </div>
        <div class="health-stat">
          <span class="health-value">{connectivityStats.connections_relayed}</span>
          <span class="health-label">Relayed</span>
        </div>
        <div class="health-stat">
          <span class="health-value">
            {connectivityStats.hole_punch_successes}/{connectivityStats.hole_punch_attempts}
          </span>
          <span class="health-label">Hole Punches</span>
        </div>
        <div class="health-stat">
          <span class="health-value">
            {connectivityStats.relay_successes}/{connectivityStats.relay_attempts}
          </span>
          <span class="health-label">Relay Reservations</span>
        </div>
        <div class="health-stat">
          <span class="health-value">{connectivityStats.dial_failures}</span>
          <span class="health-label">Dial Failures</span>
        </div>
      </div>
      <button class="refresh-btn" on:click={refreshStats} disabled={loadingStats}>
        {loadingStats ? 'Refreshing…' : '↻ Refresh'}
      </button>
    {:else if loadingStats}
      <p class="muted">Loading stats…</p>
    {:else}
      <p class="muted">No telemetry data available yet.</p>
    {/if}
  </div>

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
  .setting-label { font-size: 13px; font-weight: 500; }
  .setting-desc { font-size: 11px; color: var(--text-secondary); margin-top: 2px; }
  .setting-value { font-size: 12px; color: var(--text-secondary); margin-top: 2px; }
  .setting-value.mono, .truncated {
    font-family: 'JetBrains Mono', monospace; font-size: 11px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    max-width: 400px;
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
  .bootstrap-input {
    width: 100%; min-height: 60px; padding: 8px 10px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 4px; color: var(--text); font-size: 11px;
    font-family: 'JetBrains Mono', monospace;
    resize: vertical; outline: none; margin-top: 8px;
  }
  .bootstrap-input:focus { border-color: var(--accent); }
  .save-row {
    display: flex; align-items: center; gap: 12px; margin-top: 12px;
  }
  .save-btn {
    padding: 8px 16px; background: var(--accent); border: none;
    border-radius: 6px; color: white; font-size: 12px;
    cursor: pointer; font-weight: 500;
  }
  .save-btn:hover { opacity: 0.85; }
  .save-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .save-hint { font-size: 11px; color: var(--text-secondary); }
  .stop-btn {
    padding: 8px 16px; background: var(--danger, #e74c3c); border: none;
    border-radius: 6px; color: white; font-size: 12px;
    cursor: pointer; font-weight: 500;
  }
  .stop-btn:hover { opacity: 0.85; }
  .stop-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .status-dot {
    display: inline-block; width: 8px; height: 8px;
    border-radius: 50%; background: var(--text-secondary);
    margin-right: 6px;
  }
  .status-dot.online { background: var(--success); }
  .about-info p { font-size: 13px; line-height: 1.6; }
  .about-info .muted { color: var(--text-secondary); }
  .about-info .small { font-size: 11px; }
  .health-grid {
    display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px;
    margin-bottom: 12px;
  }
  .health-stat {
    display: flex; flex-direction: column; align-items: center;
    background: var(--bg); border-radius: 8px; padding: 12px 8px;
  }
  .health-value {
    font-size: 18px; font-weight: 600; color: var(--text);
    font-family: 'JetBrains Mono', monospace;
  }
  .health-label {
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.3px;
    color: var(--text-secondary); margin-top: 4px;
  }
  .refresh-btn {
    padding: 6px 14px; background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; color: var(--text); font-size: 11px;
    cursor: pointer;
  }
  .refresh-btn:hover { border-color: var(--accent); }
  .refresh-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .muted { color: var(--text-secondary); font-size: 12px; }
  .toggle-label {
    display: flex; align-items: center; gap: 8px; cursor: pointer;
  }
  .toggle-label input[type="checkbox"] {
    width: 16px; height: 16px; accent-color: var(--accent);
  }
  .toggle-text { font-size: 12px; color: var(--text-secondary); }
</style>
