<script lang="ts">
  import { identity, passphrase, showToast, nodeOnline } from '../stores/app';
  import { theme, toggleTheme } from '../stores/theme';

  let listenPort: string = '';
  let bootstrapPeers: string = '';
  let exportingKey = false;

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
  .status-dot {
    display: inline-block; width: 8px; height: 8px;
    border-radius: 50%; background: var(--text-secondary);
    margin-right: 6px;
  }
  .status-dot.online { background: var(--success); }
  .about-info p { font-size: 13px; line-height: 1.6; }
  .about-info .muted { color: var(--text-secondary); }
  .about-info .small { font-size: 11px; }
</style>
