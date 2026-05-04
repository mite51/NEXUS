<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { identity, passphrase as passStore, showToast } from '../stores/app';
  import { getIdentity } from '../ipc';

  export let vaultPath: string;
  const dispatch = createEventDispatcher();

  let pass = '';
  let error = '';
  let loading = false;

  async function handleUnlock() {
    error = '';
    if (!pass) { error = 'Enter your passphrase'; return; }

    loading = true;
    try {
      const info = await getIdentity(vaultPath, pass);
      identity.set(info);
      passStore.set(pass);
      showToast('✓ Vault unlocked');
      dispatch('complete');
    } catch (e: any) {
      error = 'Wrong passphrase';
    } finally {
      loading = false;
    }
  }
</script>

<div class="center-screen">
  <div class="card">
    <h1>⚡ NEXUS</h1>
    <p>Unlock your vault</p>
    <input type="password" placeholder="Passphrase" bind:value={pass}
           on:keydown={(e) => e.key === 'Enter' && handleUnlock()} autofocus />
    <button on:click={handleUnlock} disabled={loading}>
      {loading ? 'Unlocking...' : 'Unlock'}
    </button>
    {#if error}<div class="error">{error}</div>{/if}
  </div>
</div>

<style>
  .center-screen {
    display: flex; align-items: center; justify-content: center;
    width: 100%; height: 100vh;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
    padding: 32px;
    width: 400px;
    text-align: center;
  }
  .card h1 { color: var(--accent); margin-bottom: 8px; font-size: 24px; }
  .card p { color: var(--text-secondary); font-size: 14px; margin-bottom: 24px; }
  input {
    width: 100%; padding: 10px 14px;
    background: var(--bg); border: 1px solid var(--border);
    border-radius: 6px; color: var(--text);
    font-size: 14px; margin-bottom: 12px; outline: none;
  }
  input:focus { border-color: var(--accent); }
  button {
    width: 100%; padding: 10px;
    background: var(--accent); color: white;
    border: none; border-radius: 6px;
    font-size: 14px; cursor: pointer; margin-top: 8px;
  }
  button:hover { opacity: 0.85; }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
  .error { color: var(--error); font-size: 13px; margin-top: 8px; }
</style>
