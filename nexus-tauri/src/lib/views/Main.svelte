<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { identity, files, currentView, nodeOnline, didShort, passphrase, showToast } from '../stores/app';
  import { listFiles, pickFileToEncrypt, pickFilesToEncrypt, encryptFile, decryptFile, pickSaveLocation, shareFile, queueSend, deleteFile, renameFile } from '../ipc';
  import type { FileEntry, Contact } from '../ipc';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { listen } from '@tauri-apps/api/event';
  import FileGrid from '../components/FileGrid.svelte';
  import DetailPanel from '../components/DetailPanel.svelte';
  import ContactPicker from '../components/ContactPicker.svelte';
  import Sidebar from '../components/Sidebar.svelte';
  import StoreView from './StoreView.svelte';
  import Spinner from '../components/Spinner.svelte';
  import ProgressBar from '../components/ProgressBar.svelte';
  import ContactsView from './ContactsView.svelte';
  import SendQueueView from './SendQueueView.svelte';
  import SharedWithMeView from './SharedWithMeView.svelte';
  import PeersView from './PeersView.svelte';
  import SettingsView from './SettingsView.svelte';

  export let vaultPath: string;

  let selectedFile: FileEntry | null = null;
  let pickerMode: 'share' | 'send' | null = null;
  let pickerFile: FileEntry | null = null;
  let encrypting = false;
  let decrypting = false;
  let loadingFiles = true;
  let dragOver = false;
  let unlistenDrop: (() => void) | null = null;
  let unlistenProgress: (() => void) | null = null;
  let unlistenDecryptProgress: (() => void) | null = null;
  let encryptProgress = 0;
  let decryptProgress = 0;
  let searchQuery = '';
  let sortBy: 'name' | 'size' | 'shards' = 'name';

  $: filteredFiles = (() => {
    let result = searchQuery.trim()
      ? $files.filter(f => f.filename.toLowerCase().includes(searchQuery.toLowerCase()))
      : [...$files];
    if (sortBy === 'name') result.sort((a, b) => a.filename.localeCompare(b.filename));
    else if (sortBy === 'size') result.sort((a, b) => b.total_size - a.total_size);
    else result.sort((a, b) => b.shard_count - a.shard_count);
    return result;
  })();

  onMount(async () => {
    try {
      const f = await listFiles();
      files.set(f);
    } catch (e) { console.error('Failed to list files:', e); }
    loadingFiles = false;

    // Listen for drag-and-drop
    const appWindow = getCurrentWindow();
    unlistenDrop = await appWindow.onDragDropEvent(async (event) => {
      if (event.payload.type === 'over') {
        dragOver = true;
      } else if (event.payload.type === 'drop') {
        dragOver = false;
        const paths = event.payload.paths;
        if (paths && paths.length > 0) {
          for (const filePath of paths) {
            await handleEncryptPath(filePath);
          }
        }
      } else {
        dragOver = false;
      }
    });
    // Listen for encrypt progress
    unlistenProgress = await listen<{ current: number; total: number }>('nexus://encrypt-progress', (event) => {
      encryptProgress = event.payload.current / event.payload.total;
    });
    unlistenDecryptProgress = await listen<{ current: number; total: number }>('nexus://decrypt-progress', (event) => {
      decryptProgress = event.payload.current / event.payload.total;
    });
  });

  onDestroy(() => {
    if (unlistenDrop) unlistenDrop();
    if (unlistenProgress) unlistenProgress();
    if (unlistenDecryptProgress) unlistenDecryptProgress();
  });

  function handleCopyDid() {
    const id = $identity;
    if (id) {
      navigator.clipboard.writeText(id.did);
      showToast('DID copied to clipboard');
    }
  }

  async function handleEncrypt() {
    const filePaths = await pickFilesToEncrypt();
    if (!filePaths.length) return;
    for (const filePath of filePaths) {
      await handleEncryptPath(filePath);
    }
  }

  async function handleEncryptPath(filePath: string) {
    encrypting = true;
    encryptProgress = 0;
    try {
      const result = await encryptFile(filePath, vaultPath, $passphrase);
      showToast(`✓ Encrypted: ${result.filename} (${result.shard_count} shards)`);
      const f = await listFiles();
      files.set(f);
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
    encrypting = false;
    encryptProgress = 0;
  }

  function handleFileSelect(e: CustomEvent<FileEntry>) {
    selectedFile = e.detail;
  }

  async function handleDecrypt(e: CustomEvent<FileEntry>) {
    const file = e.detail;
    const savePath = await pickSaveLocation(file.filename);
    if (!savePath) return;

    decrypting = true;
    decryptProgress = 0;
    try {
      const out = await decryptFile(file.manifest_path, savePath, vaultPath, $passphrase);
      showToast(`✓ Decrypted: ${out}`);
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
    decrypting = false;
    decryptProgress = 0;
  }

  let confirmingDelete: FileEntry | null = null;

  async function handleDelete(e: CustomEvent<FileEntry>) {
    const file = e.detail;
    if (!confirm(`Delete "${file.filename}"? This removes the manifest and all shards. This cannot be undone.`)) return;
    try {
      await deleteFile(file.manifest_path);
      showToast(`✓ Deleted ${file.filename}`);
      selectedFile = null;
      const f = await listFiles();
      files.set(f);
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
  }

  async function handleRename(e: CustomEvent<FileEntry>) {
    const file = e.detail;
    const newName = prompt('New name:', file.filename);
    if (!newName || newName === file.filename) return;
    try {
      await renameFile(file.manifest_path, newName);
      showToast(`✓ Renamed to ${newName}`);
      const f = await listFiles();
      files.set(f);
      selectedFile = null;
    } catch (e: any) {
      showToast(`⚠ ${e}`);
    }
  }

  function handleShare(e: CustomEvent<FileEntry>) {
    pickerFile = e.detail;
    pickerMode = 'share';
  }

  function handleSend(e: CustomEvent<FileEntry>) {
    pickerFile = e.detail;
    pickerMode = 'send';
  }

  async function handleContactSelected(e: CustomEvent<Contact>) {
    const contact = e.detail;
    if (pickerMode === 'share' && pickerFile) {
      if (!contact.pre_public_key_hex) {
        showToast(`Cannot share: ${contact.name} has no PRE public key`);
      } else {
        try {
          const result = await shareFile(
            pickerFile.manifest_path,
            contact.did,
            contact.pre_public_key_hex,
            vaultPath,
            $passphrase
          );
          showToast(`✓ Shared with ${contact.name} → ${result.grant_path}`);
        } catch (e: any) {
          showToast(`Share error: ${e}`);
        }
      }
    } else if (pickerMode === 'send' && pickerFile) {
      const peerId = contact.did.replace('did:nexus:', '');
      try {
        await queueSend(
          pickerFile.manifest_path,
          contact.did,
          peerId
        );
        showToast(`✓ Queued send to ${contact.name} — will deliver when online`);
      } catch (e: any) {
        showToast(`Send error: ${e}`);
      }
    }
    pickerMode = null;
    pickerFile = null;
  }

  function handlePickerCancel() {
    pickerMode = null;
    pickerFile = null;
  }

  function handleKeydown(e: KeyboardEvent) {
    const ctrl = e.ctrlKey || e.metaKey;
    if (ctrl && e.key === 'e') {
      e.preventDefault();
      if ($currentView === 'files' && !encrypting) handleEncrypt();
    } else if (ctrl && e.key === 'f') {
      e.preventDefault();
      if ($currentView !== 'files') currentView.set('files');
      const input = document.querySelector('.search-input') as HTMLInputElement;
      input?.focus();
    } else if (e.key === 'Escape') {
      if (pickerMode) { handlePickerCancel(); }
      else if (selectedFile) { selectedFile = null; }
      else { searchQuery = ''; }
    }
  }
</script>

<svelte:window on:keydown={handleKeydown} />

<Sidebar
  did={$didShort}
  view={$currentView}
  online={$nodeOnline}
  fileCount={$files.length}
  on:navigate={(e) => { currentView.set(e.detail); selectedFile = null; }}
  on:copyDid={handleCopyDid}
/>

<main class="main" class:drag-over={dragOver}>
  {#if dragOver}
    <div class="drop-overlay">
      <div class="drop-content">
        <span class="drop-icon">🔒</span>
        <p>Drop to encrypt</p>
      </div>
    </div>
  {/if}
  <div class="toolbar">
    <h2>{
      $currentView === 'files' ? 'My Files' :
      $currentView === 'shared' ? 'Shared With Me' :
      $currentView === 'outbox' ? 'Outbox' :
      $currentView === 'contacts' ? 'Contacts' :
      $currentView === 'peers' ? 'Peers' :
      $currentView === 'settings' ? 'Settings' : 'Store'
    }</h2>
    <div class="spacer"></div>
    {#if $currentView === 'files'}
      <input
        class="search-input"
        type="text"
        placeholder="Search files…"
        bind:value={searchQuery}
      />
      <select class="sort-select" bind:value={sortBy}>
        <option value="name">Name</option>
        <option value="size">Size</option>
        <option value="shards">Shards</option>
      </select>
      <button class="primary-btn" on:click={handleEncrypt} disabled={encrypting}>
        {encrypting ? 'Encrypting…' : '+ Encrypt File'}
      </button>
    {/if}
  </div>

  {#if encrypting && encryptProgress > 0}
    <div class="progress-row">
      <ProgressBar progress={encryptProgress} label="Encrypting" />
    </div>
  {/if}
  {#if decrypting && decryptProgress > 0}
    <div class="progress-row">
      <ProgressBar progress={decryptProgress} label="Decrypting" />
    </div>
  {/if}

  <div class="content-wrapper">
    <div class="content">
      {#if $currentView === 'files'}
        {#if loadingFiles}
          <div class="loading-center"><Spinner size={32} /></div>
        {:else}
          <FileGrid files={filteredFiles} on:select={handleFileSelect} />
        {/if}
      {:else if $currentView === 'shared'}
        <SharedWithMeView {vaultPath} />
      {:else if $currentView === 'outbox'}
        <SendQueueView />
      {:else if $currentView === 'contacts'}
        <ContactsView />
      {:else if $currentView === 'peers'}
        <PeersView {vaultPath} />
      {:else if $currentView === 'settings'}
        <SettingsView />
      {:else}
        <StoreView />
      {/if}
    </div>

    {#if selectedFile && $currentView === 'files'}
      <DetailPanel
        file={selectedFile}
        on:close={() => selectedFile = null}
        on:decrypt={handleDecrypt}
        on:share={handleShare}
        on:send={handleSend}
        on:delete={handleDelete}
        on:rename={handleRename}
      />
    {/if}
  </div>
</main>

{#if pickerMode}
  <ContactPicker
    title={pickerMode === 'share' ? 'Share With' : 'Send To'}
    actionLabel={pickerMode === 'share' ? 'Share' : 'Send'}
    on:select={handleContactSelected}
    on:cancel={handlePickerCancel}
  />
{/if}

<style>
  .main {
    flex: 1; display: flex; flex-direction: column; overflow: hidden;
  }
  .toolbar {
    padding: 12px 24px;
    border-bottom: 1px solid var(--border);
    display: flex; align-items: center; gap: 12px;
  }
  .toolbar h2 { font-size: 16px; font-weight: 600; }
  .spacer { flex: 1; }
  .primary-btn {
    background: var(--accent); color: white;
    border: none; padding: 8px 16px; border-radius: 6px;
    font-size: 13px; cursor: pointer;
  }
  .primary-btn:hover { opacity: 0.85; }
  .content-wrapper {
    flex: 1; display: flex; overflow: hidden;
  }
  .content { flex: 1; padding: 24px; overflow-y: auto; }
  .empty {
    display: flex; flex-direction: column;
    align-items: center; justify-content: center;
    height: 100%; gap: 8px; color: var(--text-secondary);
  }
  .empty .icon { font-size: 48px; }
  .empty p { font-size: 14px; }
  .empty .hint { font-size: 12px; }
  .loading-center {
    display: flex; align-items: center; justify-content: center;
    height: 100%;
  }
  .primary-btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .search-input {
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; padding: 6px 12px;
    color: var(--text); font-size: 13px;
    outline: none; width: 200px;
    transition: border-color 0.15s;
  }
  .search-input:focus { border-color: var(--accent); }
  .search-input::placeholder { color: var(--text-secondary); }
  .sort-select {
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; padding: 6px 10px;
    color: var(--text); font-size: 12px;
    outline: none; cursor: pointer;
  }
  .sort-select:focus { border-color: var(--accent); }
  .progress-row { padding: 0 16px; }
  .main.drag-over { position: relative; }
  .drop-overlay {
    position: absolute; inset: 0; z-index: 100;
    background: rgba(99, 102, 241, 0.08);
    border: 2px dashed var(--accent);
    border-radius: 8px;
    display: flex; align-items: center; justify-content: center;
  }
  .drop-content { text-align: center; }
  .drop-icon { font-size: 48px; display: block; margin-bottom: 8px; }
  .drop-content p { font-size: 16px; color: var(--accent); font-weight: 600; }
</style>
