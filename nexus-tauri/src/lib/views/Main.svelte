<script lang="ts">
  import { onMount } from 'svelte';
  import { identity, files, currentView, nodeOnline, didShort, passphrase, showToast } from '../stores/app';
  import { listFiles, pickFileToEncrypt, encryptFile, decryptFile, pickSaveLocation, shareFile, queueSend } from '../ipc';
  import type { FileEntry, Contact } from '../ipc';
  import FileGrid from '../components/FileGrid.svelte';
  import DetailPanel from '../components/DetailPanel.svelte';
  import ContactPicker from '../components/ContactPicker.svelte';
  import Sidebar from '../components/Sidebar.svelte';
  import StoreView from './StoreView.svelte';
  import ContactsView from './ContactsView.svelte';
  import SendQueueView from './SendQueueView.svelte';
  import SharedWithMeView from './SharedWithMeView.svelte';
  import PeersView from './PeersView.svelte';

  export let vaultPath: string;

  let selectedFile: FileEntry | null = null;
  let pickerMode: 'share' | 'send' | null = null;
  let pickerFile: FileEntry | null = null;

  onMount(async () => {
    try {
      const f = await listFiles();
      files.set(f);
    } catch (e) { console.error('Failed to list files:', e); }
  });

  function handleCopyDid() {
    const id = $identity;
    if (id) {
      navigator.clipboard.writeText(id.did);
      showToast('DID copied to clipboard');
    }
  }

  async function handleEncrypt() {
    const filePath = await pickFileToEncrypt();
    if (!filePath) return;

    try {
      const result = await encryptFile(filePath, vaultPath, $passphrase);
      showToast(`✓ Encrypted: ${result.filename} (${result.shard_count} shards)`);
      const f = await listFiles();
      files.set(f);
    } catch (e: any) {
      showToast(`Error: ${e}`);
    }
  }

  function handleFileSelect(e: CustomEvent<FileEntry>) {
    selectedFile = e.detail;
  }

  async function handleDecrypt(e: CustomEvent<FileEntry>) {
    const file = e.detail;
    const savePath = await pickSaveLocation(file.filename);
    if (!savePath) return;

    try {
      const out = await decryptFile(file.manifest_path, savePath, vaultPath, $passphrase);
      showToast(`✓ Decrypted: ${out}`);
    } catch (e: any) {
      showToast(`Error: ${e}`);
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
</script>

<Sidebar
  did={$didShort}
  view={$currentView}
  online={$nodeOnline}
  on:navigate={(e) => { currentView.set(e.detail); selectedFile = null; }}
  on:copyDid={handleCopyDid}
/>

<main class="main">
  <div class="toolbar">
    <h2>{
      $currentView === 'files' ? 'My Files' :
      $currentView === 'shared' ? 'Shared With Me' :
      $currentView === 'outbox' ? 'Outbox' :
      $currentView === 'contacts' ? 'Contacts' :
      $currentView === 'peers' ? 'Peers' : 'Store'
    }</h2>
    <div class="spacer"></div>
    {#if $currentView === 'files'}
      <button class="primary-btn" on:click={handleEncrypt}>+ Encrypt File</button>
    {/if}
  </div>

  <div class="content-wrapper">
    <div class="content">
      {#if $currentView === 'files'}
        <FileGrid files={$files} on:select={handleFileSelect} />
      {:else if $currentView === 'shared'}
        <SharedWithMeView {vaultPath} />
      {:else if $currentView === 'outbox'}
        <SendQueueView />
      {:else if $currentView === 'contacts'}
        <ContactsView />
      {:else if $currentView === 'peers'}
        <PeersView {vaultPath} />
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
</style>
