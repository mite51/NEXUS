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

export interface ShardInfo {
  cid: string;
  size: number;
}

export interface VerifyResult {
  total: number;
  valid: number;
  corrupted: string[];
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

export async function listShards(): Promise<ShardInfo[]> {
  return invoke('list_shards');
}

export async function verifyStore(): Promise<VerifyResult> {
  return invoke('verify_store');
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

export async function pickFilesToEncrypt(): Promise<string[]> {
  const result = await open({
    title: 'Choose files to encrypt',
    multiple: true,
  });
  if (!result) return [];
  return Array.isArray(result) ? result : [result];
}

export async function pickBundleFile(): Promise<string | null> {
  const result = await open({
    title: 'Choose a .nexus-bundle file to import',
    multiple: false,
    filters: [{ name: 'NEXUS Bundle', extensions: ['nexus-bundle'] }],
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
  pre_seed_encrypted?: string;
  invite_pending?: boolean;
  peer_id?: string;
  relay_addrs?: string[];
  notes?: string;
}

export async function addContact(name: string, did: string, prePublicKeyHex?: string, peerId?: string, relayAddrs?: string[], notes?: string, vaultPath?: string, passphrase?: string): Promise<Contact> {
  return invoke('add_contact', { name, did, prePublicKeyHex, peerId, relayAddrs, notes, vaultPath, passphrase });
}

export async function listContacts(): Promise<Contact[]> {
  return invoke('list_contacts');
}

export async function removeContact(did: string): Promise<void> {
  return invoke('remove_contact', { did });
}

export async function getInviteKey(did: string): Promise<string> {
  return invoke('get_invite_key', { did });
}

export async function createJoinRequest(vaultPath: string, passphrase: string, name: string, includePre: boolean): Promise<string> {
  return invoke('create_join_request', { vaultPath, passphrase, name, includePre });
}

export async function acceptJoinRequest(vaultPath: string, passphrase: string, myName: string, requestJson: string): Promise<string> {
  return invoke('accept_join_request', { vaultPath, passphrase, myName, requestJson });
}

export async function applyJoinResponse(responseJson: string): Promise<string> {
  return invoke('apply_join_response', { responseJson });
}

export async function updateContact(did: string, name?: string, prePublicKeyHex?: string, peerId?: string, relayAddrs?: string[], notes?: string): Promise<Contact> {
  return invoke('update_contact', { did, name, prePublicKeyHex, peerId, relayAddrs, notes });
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

// Share Management (pull-only model)
export interface SharedUserInfo {
  did: string;
  name?: string;
}

export interface ShareInfo {
  asset_id: string;
  share_link: string;
  shared_with: SharedUserInfo[];
  public?: boolean;
}

export async function getShareInfo(manifestPath: string, peerId: string): Promise<ShareInfo> {
  return invoke('get_share_info', { manifestPath, peerId });
}

export async function revokeShare(manifestPath: string, recipientDid: string): Promise<boolean> {
  return invoke('revoke_share', { manifestPath, recipientDid });
}

// Received Files
export interface ReceivedFileInfo {
  id: string;
  sender_did: string;
  filename: string;
  has_share_grant: boolean;
  received_at: number;
  decrypted: boolean;
  total_size: number;
  shard_count: number;
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

export interface RelayServerEntry {
  name: string;
  addr: string;
}

export interface AppConfig {
  listen_port: number | null;
  bootstrap_peers: string[];
  relay_servers: RelayServerEntry[];
  telemetry_enabled: boolean;
  auto_start_node: boolean;
  auto_start_relay: boolean;
  relay_port: number;
  relay_max_circuits: number;
  use_local_relay: boolean;
}

export interface ConnectivityStats {
  hole_punch_attempts: number;
  hole_punch_successes: number;
  relay_attempts: number;
  relay_successes: number;
  dial_failures: number;
  connections_total: number;
  connections_relayed: number;
  last_nat_status: string;
}

export async function getConnectivityStats(): Promise<ConnectivityStats> {
  return invoke('get_connectivity_stats');
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

export async function renameFile(manifestPath: string, newName: string): Promise<void> {
  return invoke('rename_file', { manifestPath, newName });
}

export async function exportFileBundle(manifestPath: string, outputPath: string): Promise<string> {
  return invoke('export_file_bundle', { manifestPath, outputPath });
}

export async function importFileBundle(bundlePath: string): Promise<string> {
  return invoke('import_file_bundle', { bundlePath });
}

// --- Relay Server ---

export interface RelayInfo {
  running: boolean;
  peer_id: string | null;
  stats: {
    running: boolean;
    peer_id: string | null;
    public_ip: string | null;
    listen_addrs: string[];
    connected_peers: number;
    active_reservations: number;
    total_circuits: number;
  };
}

export async function startRelay(
  port?: number,
  maxCircuits?: number,
  maxReservationsPerPeer?: number,
): Promise<string> {
  return invoke('start_relay', { port, maxCircuits, maxReservationsPerPeer });
}

export async function stopRelay(): Promise<void> {
  return invoke('stop_relay');
}

export async function getRelayInfo(): Promise<RelayInfo> {
  return invoke('get_relay_info');
}

// --- Push send ---

export interface PushSendProgress {
  status: 'requesting' | 'streaming' | 'complete' | 'error';
  filename: string;
  shards_sent: number;
  shards_total: number;
  asset_id: string | null;
  error: string | null;
}

export async function pushToPeer(
  filePath: string,
  targetPeerId: string,
  targetFolder: string,
  vaultPath: string,
  passphrase: string,
): Promise<string> {
  return invoke('push_to_peer', { filePath, targetPeerId, targetFolder, vaultPath, passphrase });
}
