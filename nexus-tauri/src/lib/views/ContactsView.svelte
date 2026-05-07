<script lang="ts">
  import { onMount } from 'svelte';
  import { listContacts, addContact, removeContact, updateContact, getInviteKey, createJoinRequest, acceptJoinRequest, applyJoinResponse } from '../ipc';
  import type { Contact } from '../ipc';
  import { showToast } from '../stores/app';
  import { passphrase as passStore } from '../stores/app';

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

  // Join request state
  let showJoin = false;
  let joinMode: 'create' | 'accept' | 'apply' = 'create';
  let joinMyName = '';
  let joinIncludePre = true;
  let joinOutput = '';
  let joinInput = '';
  let joinError = '';

  const vaultPath = 'vault.json';

  onMount(async () => {
    try { contacts = await listContacts(); } catch (e) { console.error(e); }
  });

  // --- Join Request Handlers ---

  async function handleCreateJoinRequest() {
    joinError = ''; joinOutput = '';
    if (!joinMyName.trim()) { joinError = 'Your display name is required'; return; }
    try {
      const json = await createJoinRequest(vaultPath, $passStore, joinMyName.trim(), joinIncludePre);
      joinOutput = json;
      showToast('Join request created — copy and send to peer');
    } catch (e: any) {
      joinError = typeof e === 'string' ? e : 'Failed to create join request';
    }
  }

  async function handleAcceptJoinRequest() {
    joinError = ''; joinOutput = '';
    if (!joinMyName.trim()) { joinError = 'Your display name is required'; return; }
    if (!joinInput.trim()) { joinError = 'Paste the join request JSON'; return; }
    try {
      const resultJson = await acceptJoinRequest(vaultPath, $passStore, joinMyName.trim(), joinInput.trim());
      const result = JSON.parse(resultJson);
      joinOutput = JSON.stringify(result.response);
      contacts = await listContacts(); // refresh
      showToast('Join request accepted — send the response back to them');
    } catch (e: any) {
      joinError = typeof e === 'string' ? e : 'Failed to accept join request';
    }
  }

  async function handleApplyJoinResponse() {
    joinError = ''; joinOutput = '';
    if (!joinInput.trim()) { joinError = 'Paste the join response JSON'; return; }
    try {
      const msg = await applyJoinResponse(joinInput.trim());
      contacts = await listContacts(); // refresh
      joinInput = '';
      showToast(msg);
    } catch (e: any) {
      joinError = typeof e === 'string' ? e : 'Failed to apply join response';
    }
  }

  function copyOutput() {
    if (joinOutput) {
      navigator.clipboard.writeText(joinOutput);
      showToast('Copied to clipboard');
    }
  }

  // --- Contact Handlers ---

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
    <div class="header-actions">
      <button class="join-toggle" on:click={() => { showJoin = !showJoin; showAdd = false; }}>
        {showJoin ? 'Cancel' : '🤝 Join'}
      </button>
      <button class="add-toggle" on:click={() => { showAdd = !showAdd; showJoin = false; }}>
        {showAdd ? 'Cancel' : '+ Add'}
      </button>
    </div>
  </div>

  {#if showJoin}
    <div class="join-panel">
      <div class="join-tabs">
        <button class:active={joinMode === 'create'} on:click={() => { joinMode = 'create'; joinOutput = ''; joinError = ''; }}>
          Create Request
        </button>
        <button class:active={joinMode === 'accept'} on:click={() => { joinMode = 'accept'; joinOutput = ''; joinError = ''; }}>
          Accept Request
        </button>
        <button class:active={joinMode === 'apply'} on:click={() => { joinMode = 'apply'; joinOutput = ''; joinError = ''; }}>
          Apply Response
        </button>
      </div>

      {#if joinMode === 'create'}
        <div class="join-form">
          <input type="text" placeholder="Your display name" bind:value={joinMyName} />
          <label class="checkbox-row">
            <input type="checkbox" bind:checked={joinIncludePre} />
            <span>Include my PRE key (let them share files with me immediately)</span>
          </label>
          <button class="save-btn" on:click={handleCreateJoinRequest}>Generate Request</button>
        </div>
      {:else if joinMode === 'accept'}
        <div class="join-form">
          <input type="text" placeholder="Your display name" bind:value={joinMyName} />
          <textarea class="join-input" placeholder="Paste their join request JSON here..." bind:value={joinInput}></textarea>
          <button class="save-btn" on:click={handleAcceptJoinRequest}>Accept & Generate Response</button>
        </div>
      {:else}
        <div class="join-form">
          <textarea class="join-input" placeholder="Paste the join response JSON here..." bind:value={joinInput}></textarea>
          <button class="save-btn" on:click={handleApplyJoinResponse}>Apply Response</button>
        </div>
      {/if}

      {#if joinOutput}
        <div class="join-output">
          <div class="output-header">
            <span>{joinMode === 'create' ? 'Send this to your peer:' : 'Send this response back:'}</span>
            <button class="copy-btn" on:click={copyOutput}>📋 Copy</button>
          </div>
          <pre class="output-text">{joinOutput}</pre>
        </div>
      {/if}
      {#if joinError}<div class="error">{joinError}</div>{/if}
    </div>
  {/if}

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
            <!-- Display mode: single compact row -->
            <div class="contact-row">
              <div class="avatar-sm">
                {contact.name.charAt(0).toUpperCase()}
              </div>
              <span class="name">{contact.name}</span>
              <div class="badges">
                {#if contact.invite_pending}
                  <span class="badge invite">📨</span>
                {:else if contact.pre_public_key_hex}
                  <span class="badge ok">PRE</span>
                {:else}
                  <span class="badge warn">!PRE</span>
                {/if}
                {#if contact.peer_id}
                  <span class="badge ok">P2P</span>
                {:else}
                  <span class="badge warn">!P2P</span>
                {/if}
              </div>
              {#if contact.peer_id}
                <button class="peer-id" on:click={() => copyPeerId(contact.peer_id!)} title="Copy Peer ID">
                  {contact.peer_id.slice(0, 8)}…{contact.peer_id.slice(-4)}
                </button>
              {/if}
              {#if contact.notes}
                <span class="notes-inline">{contact.notes}</span>
              {/if}
              <div class="row-actions">
                <button class="icon-btn" on:click={() => startEdit(contact)} title="Edit">✏️</button>
                {#if confirmDelete === contact.did}
                  <button class="delete-confirm" on:click={() => handleDelete(contact.did)}>Yes</button>
                  <button class="icon-btn" on:click={() => confirmDelete = null}>✕</button>
                {:else}
                  <button class="icon-btn" on:click={() => confirmDelete = contact.did} title="Delete">🗑</button>
                {/if}
              </div>
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
  .header-actions { display: flex; gap: 8px; }
  .count { font-size: 13px; color: var(--text-secondary); }
  .add-toggle, .join-toggle {
    background: var(--accent); color: white;
    border: none; padding: 6px 14px; border-radius: 6px;
    font-size: 12px; cursor: pointer;
  }
  .join-toggle { background: var(--surface); color: var(--text); border: 1px solid var(--border); }
  .join-toggle:hover { border-color: var(--accent); }
  .add-toggle:hover { opacity: 0.85; }
  .join-panel {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 8px; padding: 16px; margin-bottom: 16px;
  }
  .join-tabs {
    display: flex; gap: 4px; margin-bottom: 12px;
  }
  .join-tabs button {
    flex: 1; padding: 6px 8px; font-size: 11px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 4px; cursor: pointer; color: var(--text-secondary);
  }
  .join-tabs button.active {
    background: var(--accent); color: white; border-color: var(--accent);
  }
  .join-form { display: flex; flex-direction: column; gap: 8px; }
  .join-form input[type="text"] {
    padding: 8px 12px; background: var(--bg);
    border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); font-size: 13px; outline: none;
  }
  .join-form input[type="text"]:focus { border-color: var(--accent); }
  .join-input {
    padding: 8px 12px; background: var(--bg);
    border: 1px solid var(--border); border-radius: 6px;
    color: var(--text); font-size: 11px; outline: none;
    font-family: 'JetBrains Mono', monospace;
    min-height: 60px; resize: vertical;
  }
  .join-input:focus { border-color: var(--accent); }
  .checkbox-row {
    display: flex; align-items: center; gap: 8px;
    font-size: 12px; color: var(--text-secondary); cursor: pointer;
  }
  .join-output {
    margin-top: 12px; background: var(--bg);
    border: 1px solid var(--border); border-radius: 6px;
    padding: 10px;
  }
  .output-header {
    display: flex; justify-content: space-between; align-items: center;
    margin-bottom: 6px;
  }
  .output-header span { font-size: 11px; color: var(--text-secondary); }
  .copy-btn {
    background: none; border: 1px solid var(--border);
    border-radius: 4px; padding: 2px 8px; font-size: 11px;
    cursor: pointer; color: var(--text-secondary);
  }
  .copy-btn:hover { border-color: var(--accent); color: var(--accent); }
  .output-text {
    font-size: 10px; font-family: 'JetBrains Mono', monospace;
    color: var(--text); white-space: pre-wrap; word-break: break-all;
    margin: 0; max-height: 120px; overflow-y: auto;
  }
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
    display: flex; align-items: center;
    padding: 8px 12px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    transition: border-color 0.15s;
  }
  .contact-card:hover { border-color: var(--accent); }
  .contact-row {
    display: flex; align-items: center; gap: 10px;
    width: 100%; min-width: 0;
  }
  .avatar-sm {
    width: 28px; height: 28px; border-radius: 50%;
    background: var(--accent); color: white;
    display: flex; align-items: center; justify-content: center;
    font-weight: 600; font-size: 12px; flex-shrink: 0;
  }
  .name { font-size: 13px; font-weight: 600; white-space: nowrap; }
  .badges { display: flex; gap: 4px; flex-shrink: 0; }
  .badge {
    display: inline-block; font-size: 9px;
    padding: 1px 5px; border-radius: 3px;
  }
  .badge.ok { background: rgba(34, 197, 94, 0.15); color: #22c55e; }
  .badge.warn { background: rgba(245, 158, 11, 0.15); color: #f59e0b; }
  .badge.invite { background: rgba(99, 102, 241, 0.15); color: #6366f1; }
  .form-hint { font-size: 11px; color: var(--text-secondary); margin-top: -4px; }
  .peer-id {
    font-size: 10px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    background: none; border: none; cursor: pointer;
    padding: 0; white-space: nowrap;
  }
  .peer-id:hover { color: var(--accent); }
  .notes-inline {
    font-size: 11px; color: var(--text-secondary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    max-width: 120px;
  }
  .row-actions { display: flex; gap: 2px; align-items: center; margin-left: auto; flex-shrink: 0; }
  .icon-btn {
    background: none; border: none; cursor: pointer;
    font-size: 13px; opacity: 0.6; transition: opacity 0.15s;
    padding: 2px 4px;
  }
  .icon-btn:hover { opacity: 1; }
  .delete-confirm {
    background: var(--error); color: white;
    border: none; padding: 2px 8px; border-radius: 4px;
    font-size: 10px; cursor: pointer;
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
