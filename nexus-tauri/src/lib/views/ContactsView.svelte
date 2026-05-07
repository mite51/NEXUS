<script lang="ts">
  import { onMount } from 'svelte';
  import { listContacts, addContact, removeContact } from '../ipc';
  import type { Contact } from '../ipc';
  import { showToast } from '../stores/app';

  let contacts: Contact[] = [];
  let showAdd = false;
  let newName = '';
  let newDid = '';
  let newPrePk = '';
  let newPeerId = '';
  let newRelayAddrs = '';
  let newNotes = '';
  let error = '';
  let confirmDelete: string | null = null;

  onMount(async () => {
    try { contacts = await listContacts(); } catch (e) { console.error(e); }
  });

  async function handleAdd() {
    error = '';
    if (!newName.trim()) { error = 'Name required'; return; }
    if (!newDid.trim() || !newDid.startsWith('did:nexus:')) {
      error = 'Invalid DID (must start with did:nexus:)';
      return;
    }

    const relayAddrs = newRelayAddrs.trim()
      ? newRelayAddrs.split('\n').map(s => s.trim()).filter(Boolean)
      : undefined;

    try {
      const contact = await addContact(
        newName.trim(),
        newDid.trim(),
        newPrePk.trim() || undefined,
        newPeerId.trim() || undefined,
        relayAddrs,
        newNotes.trim() || undefined
      );
      contacts = [...contacts, contact];
      showAdd = false;
      newName = ''; newDid = ''; newPrePk = ''; newPeerId = ''; newRelayAddrs = ''; newNotes = '';
      showToast(`Added ${contact.name}`);
    } catch (e: any) {
      error = typeof e === 'string' ? e : 'Failed to add contact';
    }
  }

  async function handleDelete(did: string) {
    const name = contacts.find(c => c.did === did)?.name ?? 'contact';
    try {
      await removeContact(did);
      contacts = contacts.filter(c => c.did !== did);
      confirmDelete = null;
      showToast(`Removed ${name}`);
    } catch (e: any) {
      showToast(`Error: ${e}`);
    }
  }

  function copyDid(did: string) {
    navigator.clipboard.writeText(did);
    showToast('DID copied');
  }

  function copyPeerId(peerId: string) {
    navigator.clipboard.writeText(peerId);
    showToast('Peer ID copied');
  }
</script>

<div class="contacts-view">
  <div class="header-row">
    <span class="count">{contacts.length} contact{contacts.length !== 1 ? 's' : ''}</span>
    <button class="add-toggle" on:click={() => showAdd = !showAdd}>
      {showAdd ? 'Cancel' : '+ Add Contact'}
    </button>
  </div>

  {#if showAdd}
    <div class="add-form">
      <input type="text" placeholder="Name *" bind:value={newName} />
      <input type="text" placeholder="did:nexus:..." bind:value={newDid} />
      <input type="text" placeholder="PRE public key hex (for sharing)" bind:value={newPrePk} />
      <input type="text" placeholder="Peer ID (12D3Koo...)" bind:value={newPeerId} />
      <textarea class="relay-input" placeholder="Relay addresses (one per line, optional)"
                bind:value={newRelayAddrs}></textarea>
      <input type="text" placeholder="Notes (optional)" bind:value={newNotes} />
      <button class="save-btn" on:click={handleAdd}>Save Contact</button>
      {#if error}<div class="error">{error}</div>{/if}
    </div>
  {/if}

  {#if contacts.length === 0 && !showAdd}
    <div class="empty">
      <span class="icon">👤</span>
      <p>No contacts yet</p>
      <p class="hint">Add contacts to share files via proxy re-encryption</p>
    </div>
  {:else}
    <div class="contact-grid">
      {#each contacts as contact}
        <div class="contact-card">
          <div class="avatar">
            {contact.name.charAt(0).toUpperCase()}
          </div>
          <div class="info">
            <div class="name">{contact.name}</div>
            <button class="did" on:click={() => copyDid(contact.did)} title="Click to copy DID">
              {contact.did.slice(0, 16)}...{contact.did.slice(-6)}
            </button>
            <div class="badges">
              {#if contact.pre_public_key_hex}
                <span class="badge ok">PRE ✓</span>
              {:else}
                <span class="badge warn">No PRE</span>
              {/if}
              {#if contact.peer_id}
                <span class="badge ok">P2P ✓</span>
              {:else}
                <span class="badge warn">No Peer</span>
              {/if}
            </div>
            {#if contact.peer_id}
              <button class="peer-id" on:click={() => copyPeerId(contact.peer_id!)} title="Click to copy Peer ID">
                🔗 {contact.peer_id.slice(0, 12)}…{contact.peer_id.slice(-6)}
              </button>
            {/if}
            {#if contact.relay_addrs && contact.relay_addrs.length > 0}
              <div class="relay-info">
                📡 {contact.relay_addrs.length} relay addr{contact.relay_addrs.length > 1 ? 's' : ''}
              </div>
            {/if}
            {#if contact.notes}
              <div class="notes">{contact.notes}</div>
            {/if}
          </div>
          <div class="actions">
            {#if confirmDelete === contact.did}
              <button class="delete-confirm" on:click={() => handleDelete(contact.did)}>
                Confirm
              </button>
              <button class="delete-cancel" on:click={() => confirmDelete = null}>
                ✕
              </button>
            {:else}
              <button class="delete-btn" on:click={() => confirmDelete = contact.did}>
                🗑
              </button>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .contacts-view { height: 100%; display: flex; flex-direction: column; }
  .header-row {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: 16px;
  }
  .count { font-size: 13px; color: var(--text-secondary); }
  .add-toggle {
    background: var(--accent); color: white;
    border: none; padding: 6px 14px; border-radius: 6px;
    font-size: 12px; cursor: pointer;
  }
  .add-toggle:hover { opacity: 0.85; }
  .add-form {
    display: flex; flex-direction: column; gap: 8px;
    padding: 16px; background: var(--surface);
    border: 1px solid var(--border); border-radius: 8px;
    margin-bottom: 16px;
  }
  .add-form input {
    padding: 8px 12px; background: var(--bg);
    border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); font-size: 13px; outline: none;
  }
  .add-form input:focus { border-color: var(--accent); }
  .relay-input {
    padding: 8px 12px; background: var(--bg);
    border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); font-size: 12px; outline: none;
    font-family: 'JetBrains Mono', monospace;
    min-height: 48px; resize: vertical;
  }
  .relay-input:focus { border-color: var(--accent); }
  .save-btn {
    padding: 8px; background: var(--accent);
    color: white; border: none; border-radius: 6px;
    font-size: 13px; cursor: pointer;
  }
  .save-btn:hover { opacity: 0.85; }
  .error { color: var(--error); font-size: 12px; }
  .contact-grid {
    display: flex; flex-direction: column; gap: 8px;
    flex: 1; overflow-y: auto;
  }
  .contact-card {
    display: flex; align-items: flex-start; gap: 12px;
    padding: 14px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    transition: border-color 0.15s;
  }
  .contact-card:hover { border-color: var(--accent); }
  .avatar {
    width: 40px; height: 40px; border-radius: 50%;
    background: var(--accent); color: white;
    display: flex; align-items: center; justify-content: center;
    font-weight: 600; font-size: 16px; flex-shrink: 0;
  }
  .info { flex: 1; min-width: 0; }
  .name { font-size: 14px; font-weight: 600; margin-bottom: 2px; }
  .did {
    font-size: 11px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    background: none; border: none; cursor: pointer;
    padding: 0; display: block;
  }
  .did:hover { color: var(--accent); }
  .badges { display: flex; gap: 6px; margin-top: 4px; }
  .badge {
    display: inline-block; font-size: 10px;
    padding: 1px 6px; border-radius: 3px;
  }
  .badge.ok { background: rgba(34, 197, 94, 0.15); color: #22c55e; }
  .badge.warn { background: rgba(245, 158, 11, 0.15); color: #f59e0b; }
  .peer-id {
    font-size: 10px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    background: none; border: none; cursor: pointer;
    padding: 0; margin-top: 3px; display: block;
  }
  .peer-id:hover { color: var(--accent); }
  .relay-info {
    font-size: 10px; color: var(--text-secondary);
    margin-top: 2px;
  }
  .notes {
    font-size: 12px; color: var(--text-secondary);
    margin-top: 4px;
  }
  .actions { flex-shrink: 0; display: flex; gap: 4px; }
  .delete-btn {
    background: none; border: none; cursor: pointer;
    font-size: 14px; opacity: 0.4; transition: opacity 0.15s;
  }
  .delete-btn:hover { opacity: 1; }
  .delete-confirm {
    background: var(--error); color: white;
    border: none; padding: 4px 10px; border-radius: 4px;
    font-size: 11px; cursor: pointer;
  }
  .delete-cancel {
    background: none; border: none;
    color: var(--text-secondary); cursor: pointer; font-size: 14px;
  }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    flex: 1; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
  .empty p { font-size: 14px; }
  .empty .hint { font-size: 12px; }
</style>
