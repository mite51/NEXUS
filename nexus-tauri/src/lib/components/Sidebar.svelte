<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  export let did: string;
  export let view: string;
  export let online: boolean;

  const dispatch = createEventDispatcher();

  const navItems = [
    { id: 'files', icon: '📁', label: 'My Files' },
    { id: 'shared', icon: '📨', label: 'Shared With Me' },
    { id: 'outbox', icon: '📤', label: 'Outbox' },
    { id: 'contacts', icon: '👤', label: 'Contacts' },
    { id: 'peers', icon: '🌐', label: 'Peers' },
    { id: 'store', icon: '📦', label: 'Store' },
    { id: 'settings', icon: '⚙️', label: 'Settings' },
  ];
</script>

<aside class="sidebar">
  <h1 class="logo">⚡ NEXUS</h1>

  <nav>
    {#each navItems as item}
      <button
        class="nav-item"
        class:active={view === item.id}
        on:click={() => dispatch('navigate', item.id)}
      >
        <span class="nav-icon">{item.icon}</span>
        <span>{item.label}</span>
      </button>
    {/each}
  </nav>

  <div class="footer">
    <div class="node-status">
      <span class="dot" class:online></span>
      {online ? 'Node online' : 'Node offline'}
    </div>
    <button class="did" on:click={() => dispatch('copyDid')} title="Click to copy DID">
      {did}
    </button>
  </div>
</aside>

<style>
  .sidebar {
    width: 220px;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex; flex-direction: column;
    padding: 16px 0;
  }
  .logo {
    font-size: 20px; padding: 0 16px 16px;
    color: var(--accent); font-weight: 600;
  }
  nav { flex: 1; }
  .nav-item {
    display: flex; align-items: center; gap: 8px;
    width: 100%; padding: 10px 16px;
    background: none; border: none; border-left: 3px solid transparent;
    color: var(--text-secondary); font-size: 14px;
    cursor: pointer; text-align: left;
    transition: all 0.15s;
  }
  .nav-item:hover { background: var(--border); color: var(--text); }
  .nav-item.active {
    background: var(--border); color: var(--text);
    border-left-color: var(--accent);
  }
  .nav-icon { font-size: 16px; }
  .footer {
    padding: 12px 16px;
    border-top: 1px solid var(--border);
  }
  .node-status {
    font-size: 12px; color: var(--text-secondary);
    margin-bottom: 8px; display: flex; align-items: center; gap: 6px;
  }
  .dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--text-secondary);
  }
  .dot.online { background: var(--success); }
  .did {
    background: none; border: none;
    font-family: 'JetBrains Mono', monospace;
    font-size: 11px; color: var(--text-secondary);
    cursor: pointer; word-break: break-all;
    text-align: left; padding: 0;
  }
  .did:hover { color: var(--accent); }
</style>
