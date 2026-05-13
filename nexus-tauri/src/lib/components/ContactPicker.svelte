<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { listContacts, addContact } from '../ipc';
  import type { Contact } from '../ipc';

  export let title = 'Select Contact';
  export let actionLabel = 'Select';

  const dispatch = createEventDispatcher();

  let contacts: Contact[] = [];
  let search = '';
  let showAdd = false;
  let newName = '';
  let newDid = '';
  let newPrePk = '';
  let error = '';

  $: filtered = contacts.filter(c =>
    c.name.toLowerCase().includes(search.toLowerCase()) ||
    c.did.toLowerCase().includes(search.toLowerCase())
  );

  onMount(async () => {
    try { contacts = await listContacts(); } catch (e) { console.error(e); }
  });

  function select(contact: Contact) {
    dispatch('select', contact);
  }

  function cancel() {
    dispatch('cancel');
  }

  async function handleAdd() {
    error = '';
    if (!newName.trim()) { error = 'Name required'; return; }
    if (!newDid.trim() || !newDid.startsWith('did:nexus:')) { error = 'Invalid DID (must start with did:nexus:)'; return; }

    try {
      const contact = await addContact(newName.trim(), newDid.trim(), newPrePk.trim() || undefined);
      contacts = [...contacts, contact];
      showAdd = false;
      newName = ''; newDid = ''; newPrePk = '';
    } catch (e: any) {
      error = typeof e === 'string' ? e : 'Failed to add contact';
    }
  }
</script>

<div class="overlay" role="presentation" on:click|self={cancel} on:keydown={(e) => e.key === 'Escape' && cancel()}>
  <div class="modal">
    <div class="modal-header">
      <h3>{title}</h3>
      <button class="close-btn" on:click={cancel}>✕</button>
    </div>

    <div class="search-row">
      <input type="text" placeholder="Search contacts..." bind:value={search} />
      <button class="add-btn" on:click={() => showAdd = !showAdd}>
        {showAdd ? '−' : '+'}
      </button>
    </div>

    {#if showAdd}
      <div class="add-form">
        <input type="text" placeholder="Name" bind:value={newName} />
        <input type="text" placeholder="did:nexus:..." bind:value={newDid} />
        <input type="text" placeholder="PRE public key (optional)" bind:value={newPrePk} />
        <button class="save-btn" on:click={handleAdd}>Add Contact</button>
        {#if error}<div class="error">{error}</div>{/if}
      </div>
    {/if}

    <div class="contact-list">
      {#if filtered.length === 0}
        <div class="empty-contacts">
          {contacts.length === 0 ? 'No contacts yet — add one above' : 'No matches'}
        </div>
      {:else}
        {#each filtered as contact}
          <button class="contact-row" on:click={() => select(contact)}>
            <div class="contact-avatar">
              {contact.name.charAt(0).toUpperCase()}
            </div>
            <div class="contact-info">
              <div class="contact-name">{contact.name}</div>
              <div class="contact-did">
                {contact.did.slice(0, 16)}...{contact.did.slice(-6)}
              </div>
            </div>
            <span class="select-label">{actionLabel}</span>
          </button>
        {/each}
      {/if}
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex; align-items: center; justify-content: center;
    z-index: 200;
  }
  .modal {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    width: 420px; max-height: 80vh;
    display: flex; flex-direction: column;
    overflow: hidden;
  }
  .modal-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border);
  }
  .modal-header h3 { font-size: 16px; font-weight: 600; }
  .close-btn {
    background: none; border: none;
    color: var(--text-secondary); cursor: pointer;
    font-size: 18px;
  }
  .close-btn:hover { color: var(--text); }
  .search-row {
    display: flex; gap: 8px;
    padding: 12px 20px;
  }
  .search-row input {
    flex: 1; padding: 8px 12px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; color: var(--text);
    font-size: 13px; outline: none;
  }
  .search-row input:focus { border-color: var(--accent); }
  .add-btn {
    width: 36px; height: 36px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; color: var(--accent);
    font-size: 20px; cursor: pointer;
    display: flex; align-items: center; justify-content: center;
  }
  .add-btn:hover { border-color: var(--accent); }
  .add-form {
    padding: 0 20px 12px;
    display: flex; flex-direction: column; gap: 8px;
  }
  .add-form input {
    padding: 8px 12px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; color: var(--text);
    font-size: 13px; outline: none;
  }
  .add-form input:focus { border-color: var(--accent); }
  .save-btn {
    padding: 8px; background: var(--accent);
    color: white; border: none; border-radius: 6px;
    font-size: 13px; cursor: pointer;
  }
  .save-btn:hover { opacity: 0.85; }
  .error { color: var(--error); font-size: 12px; }
  .contact-list {
    flex: 1; overflow-y: auto;
    padding: 0 8px 12px;
  }
  .empty-contacts {
    text-align: center; padding: 32px;
    color: var(--text-secondary); font-size: 13px;
  }
  .contact-row {
    display: flex; align-items: center; gap: 12px;
    width: 100%; padding: 10px 12px;
    background: none; border: none; border-radius: 8px;
    cursor: pointer; text-align: left;
    transition: background 0.15s;
  }
  .contact-row:hover { background: var(--border); }
  .contact-avatar {
    width: 36px; height: 36px;
    border-radius: 50%;
    background: var(--accent);
    color: white; font-weight: 600;
    display: flex; align-items: center; justify-content: center;
    font-size: 14px; flex-shrink: 0;
  }
  .contact-info { flex: 1; min-width: 0; }
  .contact-name { font-size: 14px; font-weight: 500; color: var(--text); }
  .contact-did {
    font-size: 11px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
  }
  .select-label {
    font-size: 12px; color: var(--accent);
    opacity: 0; transition: opacity 0.15s;
  }
  .contact-row:hover .select-label { opacity: 1; }
</style>
