import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';

let permissionGranted = false;

export async function initNotifications(): Promise<void> {
  permissionGranted = await isPermissionGranted();
  if (!permissionGranted) {
    const permission = await requestPermission();
    permissionGranted = permission === 'granted';
  }
}

export async function notifyFileReceived(filename: string, from: string): Promise<void> {
  if (!permissionGranted) return;
  sendNotification({
    title: 'File Received',
    body: `${filename} from ${from.length > 20 ? from.slice(0, 16) + '…' : from}`,
  });
}

export async function notifyTransferComplete(filename: string): Promise<void> {
  if (!permissionGranted) return;
  sendNotification({
    title: 'Transfer Complete',
    body: `${filename} delivered successfully`,
  });
}
