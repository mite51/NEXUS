import { writable, derived } from 'svelte/store';
import type { IdentityInfo, FileEntry } from '../lib/ipc';

export const identity = writable<IdentityInfo | null>(null);
export const passphrase = writable<string>('');
export const files = writable<FileEntry[]>([]);
export const currentView = writable<'files' | 'shared' | 'peers' | 'store'>('files');
export const nodeOnline = writable<boolean>(false);
export const toast = writable<string | null>(null);

export const isUnlocked = derived(identity, $id => $id !== null);
export const didShort = derived(identity, $id => {
  if (!$id) return '';
  const d = $id.did;
  return d.length > 30 ? d.slice(0, 20) + '...' + d.slice(-8) : d;
});

export function showToast(msg: string, durationMs = 3000) {
  toast.set(msg);
  setTimeout(() => toast.set(null), durationMs);
}
