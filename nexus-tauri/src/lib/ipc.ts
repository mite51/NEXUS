import { invoke } from '@tauri-apps/api/core';
import { open, save } from '@tauri-apps/plugin-dialog';

// Types matching Tauri backend responses
export interface IdentityInfo {
  did: string;
  pre_public_key_hex: string;
  peer_id: string;
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
  total_size: number;
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

// Send Queue
export interface QueuedSendInfo {
  id: string;
  recipient_did: string;
  recipient_peer_id: string;
  filename: string;
  status: string;
  queued_at: number;
  attempts: number;
}

export async function queueSend(
  manifestPath: string,
  recipientDid: string,
  recipientPeerId: string,
  recipientAddr?: string,
  shareGrantJson?: string
): Promise<QueuedSendInfo> {
  return invoke('queue_send', { manifestPath, recipientDid, recipientPeerId, recipientAddr, shareGrantJson });
}

export async function listSendQueue(): Promise<QueuedSendInfo[]> {
  return invoke('list_send_queue');
}

export async function cancelSend(id: string): Promise<void> {
  return invoke('cancel_send', { id });
}

export async function retrySend(id: string): Promise<void> {
  return invoke('retry_send', { id });
}

// Received Files
export interface ReceivedFileInfo {
  id: string;
  sender_did: string;
  filename: string;
  has_share_grant: boolean;
  received_at: number;
  decrypted: boolean;
}

export async function listReceivedFiles(): Promise<ReceivedFileInfo[]> {
  return invoke('list_received_files');
}

export async function decryptReceived(receivedId: string, vaultPath: string, passphrase: string, outputPath?: string): Promise<string> {
  return invoke('decrypt_received', { receivedId, vaultPath, passphrase, outputPath });
}

export async function removeReceived(id: string): Promise<void> {
  return invoke('remove_received', { id });
}

// Node lifecycle
export interface NodeInfo {
  running: boolean;
  peer_id: string | null;
  listen_addrs: string[];
  connected_peers: string[];
}

export async function startNode(vaultPath: string, passphrase: string, listenPort?: number): Promise<string> {
  return invoke('start_node', { vaultPath, passphrase, listenPort });
}

export async function stopNode(): Promise<void> {
  return invoke('stop_node');
}

export async function getNodeInfo(): Promise<NodeInfo> {
  return invoke('get_node_info');
}

export interface AppConfig {
  listen_port: number | null;
  bootstrap_peers: string[];
}

export async function getConfig(): Promise<AppConfig> {
  return invoke('get_config');
}

export async function saveConfig(config: AppConfig): Promise<void> {
  return invoke('save_config', { config });
}

export async function deleteFile(manifestPath: string): Promise<void> {
  return invoke('delete_file', { manifestPath });
}
