<script lang="ts">
  import { onMount } from 'svelte';
  import { listContacts, addContact, removeContact, updateContact } from '../ipc';
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

  // Edit state
  let editingDid: string | null = null;
  let editName = '';
  let editPrePk = '';
  let editPeerId = '';
  let editRelayAddrs = '';
  let editNotes = '';
  let editError = '';

  onMount(async () => {
    try { contacts = await listContacts(); } catch (e) { console.error(e); }
  });

  async function handleAdd() {
    error = '';
    if (!newName.trim()) { error = 'Name required'; return; }

    // DID is optional for invite flow — generate a placeholder if blank
    let did = newDid.trim();
    if (did && !did.startsWith('did:nexus:')) {
      error = 'Invalid DID (must start with did:nexus: or leave blank for invite)';
      return;
    }
    if (!did) {
      // Generate a temporary DID placeholder for invite contacts
      did = `did:nexus:invite-${Date.now().toString(36)}`;
    }

    const relayAddrs = newRelayAddrs.trim()
      ? newRelayAddrs.split('\n').map(s => s.trim()).filter(Boolean)
      : undefined;

    try {
      const contact = await addContact(
        newName.trim(),
        did,
        newPrePk.trim() || undefined,
        newPeerId.trim() || undefined,
        relayAddrs,
        newNotes.trim() || undefined
      );
      contacts = [...contacts, contact];
      showAdd = false;
      newName = ''; newDid = ''; newPrePk = ''; newPeerId = ''; newRelayAddrs = ''; newNotes = '';
      showToast(`Added ${contact.name}${contact.invite_pending ? ' (invite)' : ''}`);
    } catch (e: any) {
      error = typeof e === 'string' ? e : 'Failed to add contact';
    }
  }

  function startEdit(contact: Contact) {
    editingDid = contact.did;
    editName = contact.name;
    editPrePk = contact.pre_public_key_hex ?? '';
    editPeerId = contact.peer_id ?? '';
    editRelayAddrs = (contact.relay_addrs ?? []).join('\n');
    editNotes = contact.notes ?? '';
    editError = '';
  }

  function cancelEdit() {
    editingDid = null;
    editError = '';
  }

  async function handleSaveEdit() {
    if (!editingDid) return;
    editError = '';
    if (!editName.trim()) { editError = 'Name required'; return; }

    const relayAddrs = editRelayAddrs.trim()
      ? editRelayAddrs.split('\n').map(s => s.trim()).filter(Boolean)
      : undefined;

    try {
      const updated = await updateContact(
        editingDid,
        editName.trim(),
        editPrePk.trim() || undefined,
        editPeerId.trim() || undefined,
        relayAddrs,
        editNotes.trim() || undefined
      );
      contacts = contacts.map(c => c.did === editingDid ? updated : c);
      editingDid = null;
      showToast('Contact updated');
    } catch (e: any) {
      editError = typeof e === 'string' ? e : 'Update failed';
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
      <input type="text" placeholder="did:nexus:... (or leave blank for invite)" bind:value={newDid} />
      <input type="text" placeholder="PRE public key hex (leave blank to auto-generate)" bind:value={newPrePk} />
      <div class="form-hint">No PRE key? One will be generated for them (invite mode)</div>
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
          {#if editingDid === contact.did}
            <!-- Edit mode -->
            <div class="edit-form">
              <input type="text" placeholder="Name *" bind:value={editName} />
              <div class="edit-did mono">{contact.did}</div>
              <input type="text" placeholder="PRE public key hex" bind:value={editPrePk} />
              <input type="text" placeholder="Peer ID (12D3Koo...)" bind:value={editPeerId} />
              <textarea class="relay-input" placeholder="Relay addresses (one per line)"
                        bind:value={editRelayAddrs}></textarea>
              <input type="text" placeholder="Notes" bind:value={editNotes} />
              <div class="edit-actions">
                <button class="save-btn" on:click={handleSaveEdit}>Save</button>
                <button class="cancel-btn" on:click={cancelEdit}>Cancel</button>
              </div>
              {#if editError}<div class="error">{editError}</div>{/if}
            </div>
          {:else}
            <!-- Display mode -->
            <div class="avatar">
              {contact.name.charAt(0).toUpperCase()}
            </div>
            <div class="info">
              <div class="name">{contact.name}</div>
              <button class="did" on:click={() => copyDid(contact.did)} title="Click to copy DID">
                {contact.did.slice(0, 16)}...{contact.did.slice(-6)}
              </button>
              <div class="badges">
                {#if contact.invite_pending}
                  <span class="badge invite">📨 Invite</span>
                {:else if contact.pre_public_key_hex}
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
              <button class="edit-btn" on:click={() => startEdit(contact)} title="Edit">
                ✏️
              </button>
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
          {/if}
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
  .add-form, .edit-form {
    display: flex; flex-direction: column; gap: 8px;
    padding: 16px; background: var(--surface);
    border: 1px solid var(--border); border-radius: 8px;
    margin-bottom: 16px;
  }
  .edit-form {
    width: 100%; margin-bottom: 0; padding: 12px;
    background: var(--bg); border: 1px solid var(--accent);
  }
  .add-form input, .edit-form input {
    padding: 8px 12px; background: var(--bg);
    border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); font-size: 13px; outline: none;
  }
  .edit-form input {
    background: var(--surface);
  }
  .add-form input:focus, .edit-form input:focus { border-color: var(--accent); }
  .edit-did {
    font-size: 11px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    padding: 4px 0; overflow: hidden; text-overflow: ellipsis;
  }
  .relay-input {
    padding: 8px 12px; background: var(--bg);
    border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); font-size: 12px; outline: none;
    font-family: 'JetBrains Mono', monospace;
    min-height: 48px; resize: vertical;
  }
  .edit-form .relay-input { background: var(--surface); }
  .relay-input:focus { border-color: var(--accent); }
  .save-btn {
    padding: 8px; background: var(--accent);
    color: white; border: none; border-radius: 6px;
    font-size: 13px; cursor: pointer;
  }
  .save-btn:hover { opacity: 0.85; }
  .cancel-btn {
    padding: 8px; background: var(--bg);
    color: var(--text); border: 1px solid var(--border);
    border-radius: 6px; font-size: 13px; cursor: pointer;
  }
  .cancel-btn:hover { border-color: var(--accent); }
  .edit-actions {
    display: flex; gap: 8px;
  }
  .edit-actions .save-btn, .edit-actions .cancel-btn {
    flex: 1;
  }
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
  .badge.invite { background: rgba(99, 102, 241, 0.15); color: #6366f1; }
  .form-hint { font-size: 11px; color: var(--text-secondary); margin-top: -4px; }
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
  .actions { flex-shrink: 0; display: flex; gap: 4px; align-items: flex-start; }
  .edit-btn {
    background: none; border: none; cursor: pointer;
    font-size: 14px; opacity: 0.4; transition: opacity 0.15s;
  }
  .edit-btn:hover { opacity: 1; }
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
