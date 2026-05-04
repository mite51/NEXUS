import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

// Types matching Tauri backend responses
export interface IdentityInfo {
  did: string;
  pre_public_key_hex: string;
}

export interface EncryptResult {
  filename: string;
  shard_count: number;
  manifest_path: string;
}

export interface StoreStatsResult {
  shard_count: number;
  total_bytes: number;
}

export interface FileEntry {
  filename: string;
  manifest_path: string;
  owner: string;
  shard_count: number;
}

// Typed IPC wrappers
export async function createIdentity(vaultPath: string, passphrase: string): Promise<IdentityInfo> {
  return invoke('create_identity', { vaultPath, passphrase });
}

export async function getIdentity(vaultPath: string, passphrase: string): Promise<IdentityInfo> {
  return invoke('get_identity', { vaultPath, passphrase });
}

export async function encryptFile(filePath: string, vaultPath: string, passphrase: string): Promise<EncryptResult> {
  return invoke('encrypt_file', { filePath, vaultPath, passphrase });
}

export async function decryptFile(manifestPath: string, outputPath: string | null, vaultPath: string, passphrase: string): Promise<string> {
  return invoke('decrypt_file', { manifestPath, outputPath, vaultPath, passphrase });
}

export async function getStoreStats(): Promise<StoreStatsResult> {
  return invoke('get_store_stats');
}

export async function listFiles(): Promise<FileEntry[]> {
  return invoke('list_files');
}

// File dialogs
export async function pickFileToEncrypt(): Promise<string | null> {
  const result = await open({
    title: 'Choose a file to encrypt',
    multiple: false,
  });
  return result ?? null;
}

export async function pickSaveLocation(defaultName: string): Promise<string | null> {
  const result = await save({
    title: 'Save decrypted file',
    defaultPath: defaultName,
  });
  return result ?? null;
}

// Contacts
export interface Contact {
  name: string;
  did: string;
  pre_public_key_hex?: string;
  notes?: string;
}

export async function addContact(name: string, did: string, prePublicKeyHex?: string, notes?: string): Promise<Contact> {
  return invoke('add_contact', { name, did, prePublicKeyHex, notes });
}

export async function listContacts(): Promise<Contact[]> {
  return invoke('list_contacts');
}

export async function removeContact(did: string): Promise<void> {
  return invoke('remove_contact', { did });
}

export async function updateContact(did: string, name?: string, prePublicKeyHex?: string, notes?: string): Promise<Contact> {
  return invoke('update_contact', { did, name, prePublicKeyHex, notes });
}

export interface ShareResult {
  grant_path: string;
  recipient: string;
  cfrags_count: number;
}

export async function shareFile(
  manifestPath: string,
  recipientDid: string,
  recipientPrePkHex: string,
  vaultPath: string,
  passphrase: string
): Promise<ShareResult> {
  return invoke('share_file', { manifestPath, recipientDid, recipientPrePkHex, vaultPath, passphrase });
}
