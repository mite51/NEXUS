<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { listen } from '@tauri-apps/api/event';
  import { open } from '@tauri-apps/plugin-dialog';
  import { listContacts, pushToPeer } from '../ipc';
  import type { Contact, PushSendProgress } from '../ipc';
  import { passphrase, showToast } from '../stores/app';
  import ProgressBar from './ProgressBar.svelte';
  import Spinner from './Spinner.svelte';

  export let vaultPath: string;
  export let show = false;

  const dispatch = createEventDispatcher();

  let contacts: Contact[] = [];
  let selectedContact: Contact | null = null;
  let selectedFile: string | null = null;
  let targetFolder = '/';
  let pushing = false;
  let progress: PushSendProgress | null = null;
  let unlistenProgress: (() => void) | null = null;

  onMount(async () => {
    await loadContacts();
    unlistenProgress = await listen<PushSendProgress>('nexus://push-send-progress', (event) => {
      progress = event.payload;
    });
    return () => {
      if (unlistenProgress) unlistenProgress();
    };
  });

  async function loadContacts() {
    try {
      const all = await listContacts();
      // Only show contacts with a peer_id (required for push)
      contacts = all.filter(c => c.peer_id && !c.invite_pending);
    } catch (e) {
      console.error('Failed to load contacts:', e);
    }
  }

  async function pickFile() {
    const result = await open({
      title: 'Select file to push',
      multiple: false,
    });
    if (result) {
      selectedFile = result as string;
    }
  }

  async function handlePush() {
    if (!selectedContact?.peer_id || !selectedFile) return;

    pushing = true;
    progress = null;

    try {
      const assetId = await pushToPeer(
        selectedFile,
        selectedContact.peer_id,
        targetFolder,
        vaultPath,
        $passphrase,
      );
      showToast(`✓ Pushed to ${selectedContact.name} (asset: ${assetId.slice(0, 12)}…)`);
      close();
    } catch (e: any) {
      showToast(`⚠ Push failed: ${e}`);
      progress = null;
    }
    pushing = false;
  }

  function close() {
    show = false;
    selectedFile = null;
    selectedContact = null;
    targetFolder = '/';
    progress = null;
    dispatch('close');
  }
</script>

{#if show}
  <div class="modal-backdrop" transition:fade={{ duration: 150 }} on:click={close} on:keydown={e => e.key === 'Escape' && close()}>
    <div class="modal" on:click|stopPropagation on:keydown|stopPropagation>
      <header>
        <h3>📤 Push File</h3>
        <button class="close-btn" on:click={close}>✕</button>
      </header>

      <div class="body">
        <!-- File selection -->
        <div class="field">
          <label>File</label>
          <button class="file-pick" on:click={pickFile} disabled={pushing}>
            {#if selectedFile}
              <span class="filename">{selectedFile.split(/[\\/]/).pop()}</span>
            {:else}
              <span class="placeholder">Choose file…</span>
            {/if}
          </button>
        </div>

        <!-- Contact selection -->
        <div class="field">
          <label>Send to</label>
          <select bind:value={selectedContact} disabled={pushing}>
            <option value={null}>Select contact…</option>
            {#each contacts as contact}
              <option value={contact}>{contact.name}</option>
            {/each}
          </select>
        </div>

        <!-- Target folder -->
        <div class="field">
          <label>Folder</label>
          <input type="text" bind:value={targetFolder} placeholder="/" disabled={pushing} />
        </div>

        <!-- Progress -->
        {#if progress}
          <div class="progress-area">
            {#if progress.status === 'requesting'}
              <div class="status"><Spinner size={14} /> Requesting push…</div>
            {:else if progress.status === 'streaming'}
              <div class="status">Streaming shards…</div>
              <ProgressBar value={progress.shards_sent} max={progress.shards_total} />
              <div class="detail">{progress.shards_sent}/{progress.shards_total} shards</div>
            {:else if progress.status === 'complete'}
              <div class="status success">✓ Push complete</div>
            {/if}
          </div>
        {/if}
      </div>

      <footer>
        <button class="btn secondary" on:click={close} disabled={pushing}>Cancel</button>
        <button
          class="btn primary"
          on:click={handlePush}
          disabled={pushing || !selectedFile || !selectedContact}
        >
          {#if pushing}
            <Spinner size={14} /> Pushing…
          {:else}
            Push
          {/if}
        </button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .modal {
    background: var(--bg-secondary, #1e1e2e);
    border-radius: 12px;
    padding: 0;
    width: 420px;
    max-width: 90vw;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
    border: 1px solid var(--border-color, #333);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color, #333);
  }

  header h3 {
    margin: 0;
    font-size: 1.1rem;
  }

  .close-btn {
    background: none;
    border: none;
    color: var(--text-secondary, #888);
    font-size: 1.2rem;
    cursor: pointer;
    padding: 4px 8px;
    border-radius: 4px;
  }
  .close-btn:hover {
    background: var(--bg-hover, #2a2a3e);
  }

  .body {
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .field label {
    font-size: 0.85rem;
    color: var(--text-secondary, #888);
    font-weight: 500;
  }

  .file-pick {
    background: var(--bg-tertiary, #2a2a3e);
    border: 1px dashed var(--border-color, #444);
    border-radius: 8px;
    padding: 12px 16px;
    cursor: pointer;
    text-align: left;
    color: var(--text-primary, #fff);
  }
  .file-pick:hover:not(:disabled) {
    border-color: var(--accent, #7c3aed);
  }
  .file-pick .placeholder {
    color: var(--text-secondary, #888);
  }
  .file-pick .filename {
    font-weight: 500;
  }

  select, input[type="text"] {
    background: var(--bg-tertiary, #2a2a3e);
    border: 1px solid var(--border-color, #444);
    border-radius: 8px;
    padding: 10px 12px;
    color: var(--text-primary, #fff);
    font-size: 0.9rem;
  }
  select:focus, input:focus {
    outline: none;
    border-color: var(--accent, #7c3aed);
  }

  .progress-area {
    background: var(--bg-tertiary, #2a2a3e);
    border-radius: 8px;
    padding: 12px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 0.85rem;
    margin-bottom: 8px;
  }
  .status.success {
    color: #4ade80;
  }
  .detail {
    font-size: 0.8rem;
    color: var(--text-secondary, #888);
    margin-top: 4px;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 16px 20px;
    border-top: 1px solid var(--border-color, #333);
  }

  .btn {
    padding: 8px 16px;
    border-radius: 8px;
    border: none;
    font-size: 0.9rem;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .btn.primary {
    background: var(--accent, #7c3aed);
    color: white;
  }
  .btn.primary:hover:not(:disabled) {
    background: var(--accent-hover, #6d28d9);
  }
  .btn.secondary {
    background: var(--bg-tertiary, #2a2a3e);
    color: var(--text-primary, #fff);
  }
  .btn.secondary:hover:not(:disabled) {
    background: var(--bg-hover, #3a3a4e);
  }
</style>
