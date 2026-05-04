<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { identity, passphrase as passStore, showToast } from '../stores/app';
  import { createIdentity } from '../ipc';

  export let vaultPath: string;
  const dispatch = createEventDispatcher();

  let pass1 = '';
  let pass2 = '';
  let error = '';
  let loading = false;

  async function handleCreate() {
    error = '';
    if (!pass1 || pass1.length < 4) { error = 'Passphrase must be at least 4 characters'; return; }
    if (pass1 !== pass2) { error = 'Passphrases don\'t match'; return; }

    loading = true;
    try {
      const info = await createIdentity(vaultPath, pass1);
      identity.set(info);
      passStore.set(pass1);
      showToast('✓ Identity created');
      dispatch('complete');
    } catch (e: any) {
      error = typeof e === 'string' ? e : 'Failed to create identity';
    } finally {
      loading = false;
    }
  }
</script>

<div class="center-screen">
  <div class="card">
    <h1>⚡ NEXUS</h1>
    <p>Create your identity to get started</p>
    <input type="password" placeholder="Choose a passphrase" bind:value={pass1} />
    <input type="password" placeholder="Confirm passphrase" bind:value={pass2}
           on:keydown={(e) => e.key === 'Enter' && handleCreate()} />
    <button on:click={handleCreate} disabled={loading}>
      {loading ? 'Creating...' : 'Create Identity'}
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
