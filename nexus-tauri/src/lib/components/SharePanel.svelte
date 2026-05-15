<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { FileEntry, ShareInfo } from '../ipc';

  export let file: FileEntry;
  export let info: ShareInfo;

  const dispatch = createEventDispatcher();

  let copied = false;

  function copyLink() {
    navigator.clipboard.writeText(info.share_link);
    copied = true;
    setTimeout(() => copied = false, 2000);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="share-overlay" on:click|self={() => dispatch('close')}>
  <div class="share-panel">
    <div class="share-header">
      <h3>🔗 Share: {file.filename}</h3>
      <button class="close-btn" on:click={() => dispatch('close')}>✕</button>
    </div>

    <div class="share-section">
      <span class="section-label">Share Link</span>
      <div class="link-row">
        <code class="link-text">{info.share_link}</code>
        <button class="copy-btn" on:click={copyLink}>
          {copied ? '✓' : '📋'}
        </button>
      </div>
      <p class="hint">Recipients use this link to pull the file from your node when you're online.</p>
    </div>

    <div class="share-section">
      <span class="section-label">Access</span>
      <div class="access-toggle">
        <button
          class="toggle-btn"
          class:active={info.public !== true}
          on:click={() => dispatch('setPublic', false)}
        >🔒 Private</button>
        <button
          class="toggle-btn"
          class:active={info.public === true}
          on:click={() => dispatch('setPublic', true)}
        >🌐 Public</button>
      </div>
      {#if info.public}
        <p class="hint">Anyone with the link can pull this file — no authorization required.</p>
      {:else}
        <p class="hint">Only authorized users can pull this file.</p>
      {/if}
    </div>

    {#if !info.public}
      <div class="share-section">
        <span class="section-label">Authorized Users ({info.shared_with.length})</span>
        {#if info.shared_with.length === 0}
          <p class="empty">No users yet. Add a contact to grant access.</p>
        {:else}
          <div class="user-list-scroll">
            <ul class="user-list">
              {#each info.shared_with as user}
                <li class="user-row">
                  <span class="user-name">{user.name || user.did}</span>
                  {#if user.name}
                    <span class="user-did">{user.did.slice(0, 20)}…</span>
                  {/if}
                  <button class="revoke-btn" on:click={() => dispatch('revoke', user.did)} title="Revoke access">✕</button>
                </li>
              {/each}
            </ul>
          </div>
        {/if}
        <button class="add-btn" on:click={() => dispatch('addUser')}>+ Add User</button>
      </div>
    {/if}
  </div>
</div>

<style>
  .share-overlay {
    position: fixed; inset: 0;
    background: rgba(0,0,0,0.5);
    display: flex; align-items: center; justify-content: center;
    z-index: 100;
  }
  .share-panel {
    background: var(--surface, #1e1e2e);
    border: 1px solid var(--border, #333);
    border-radius: 12px;
    padding: 24px;
    width: 420px; max-width: 90vw;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 8px 32px rgba(0,0,0,0.4);
  }
  .share-header {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: 20px;
  }
  .share-header h3 {
    font-size: 16px; font-weight: 600; margin: 0;
  }
  .close-btn {
    background: none; border: none; color: var(--text-secondary, #888);
    cursor: pointer; font-size: 18px; padding: 4px 8px;
  }
  .close-btn:hover { color: var(--text, #eee); }
  .share-section {
    margin-bottom: 20px;
  }
  .share-section .section-label {
    font-size: 12px; font-weight: 600;
    text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-secondary, #888);
    display: block; margin-bottom: 8px;
  }
  .link-row {
    display: flex; align-items: center; gap: 8px;
    background: var(--bg, #11111b);
    border: 1px solid var(--border, #333);
    border-radius: 6px; padding: 8px 12px;
  }
  .link-text {
    flex: 1; font-size: 11px;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    color: var(--accent, #89b4fa);
  }
  .copy-btn {
    background: none; border: none; cursor: pointer;
    font-size: 14px; padding: 2px 6px;
    opacity: 0.7; transition: opacity 0.15s;
  }
  .copy-btn:hover { opacity: 1; }
  .hint {
    font-size: 11px; color: var(--text-secondary, #888);
    margin: 6px 0 0 0;
  }
  .access-toggle {
    display: flex; gap: 4px;
    background: var(--bg, #11111b);
    border: 1px solid var(--border, #333);
    border-radius: 6px;
    padding: 3px;
  }
  .toggle-btn {
    flex: 1;
    background: none;
    border: none;
    border-radius: 4px;
    padding: 6px 12px;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    color: var(--text-secondary, #888);
    transition: all 0.15s;
  }
  .toggle-btn.active {
    background: var(--surface, #1e1e2e);
    color: var(--text, #eee);
    box-shadow: 0 1px 3px rgba(0,0,0,0.2);
  }
  .empty {
    font-size: 13px; color: var(--text-secondary, #888);
    font-style: italic; margin: 8px 0;
  }
  .user-list-scroll {
    max-height: 160px;
    overflow-y: auto;
    border: 1px solid var(--border, #333);
    border-radius: 6px;
    margin-bottom: 12px;
  }
  .user-list {
    list-style: none; padding: 0; margin: 0;
  }
  .user-row {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border, #333);
    transition: background 0.1s;
  }
  .user-row:last-child { border-bottom: none; }
  .user-row:hover { background: var(--bg, #11111b); }
  .user-name {
    font-size: 13px; font-weight: 500;
  }
  .user-did {
    font-size: 11px; color: var(--text-secondary, #888);
    flex: 1;
  }
  .revoke-btn {
    background: none; border: none;
    color: var(--text-secondary, #888); cursor: pointer;
    font-size: 14px; padding: 2px 6px;
    border-radius: 4px;
    transition: color 0.15s, background 0.15s;
  }
  .revoke-btn:hover {
    color: #f38ba8; background: rgba(243,139,168,0.1);
  }
  .add-btn {
    background: var(--accent, #89b4fa);
    color: var(--bg, #11111b);
    border: none; border-radius: 6px;
    padding: 8px 16px;
    font-size: 13px; font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s;
  }
  .add-btn:hover { opacity: 0.9; }
</style>
