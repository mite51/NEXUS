/** Format bytes into human-readable string */
export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / Math.pow(1024, i);
  return `${val < 10 ? val.toFixed(1) : Math.round(val)} ${units[i]}`;
}

/** Format a timestamp (seconds since epoch) to relative time */
export function timeAgo(ts: number): string {
  const now = Math.floor(Date.now() / 1000);
  const diff = now - ts;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 604800) return `${Math.floor(diff / 86400)}d ago`;
  return new Date(ts * 1000).toLocaleDateString();
}

/** Truncate a string with ellipsis */
export function truncate(s: string, max: number): string {
  return s.length <= max ? s : s.slice(0, max - 1) + '…';
}

/** Get a file type category from extension */
export function fileCategory(filename: string): 'image' | 'video' | 'document' | 'code' | 'archive' | 'other' {
  const ext = filename.split('.').pop()?.toLowerCase() || '';
  if (['jpg', 'jpeg', 'png', 'gif', 'webp', 'svg', 'bmp', 'ico'].includes(ext)) return 'image';
  if (['mp4', 'mkv', 'avi', 'mov', 'webm'].includes(ext)) return 'video';
  if (['pdf', 'doc', 'docx', 'txt', 'md', 'rtf', 'odt', 'xls', 'xlsx', 'csv'].includes(ext)) return 'document';
  if (['rs', 'ts', 'js', 'py', 'go', 'c', 'cpp', 'h', 'java', 'json', 'yaml', 'toml', 'html', 'css', 'sh'].includes(ext)) return 'code';
  if (['zip', 'tar', 'gz', 'bz2', 'xz', '7z', 'rar'].includes(ext)) return 'archive';
  return 'other';
}

/** Icon for file category */
export function fileIcon(filename: string): string {
  const cat = fileCategory(filename);
  switch (cat) {
    case 'image': return '🖼️';
    case 'video': return '🎬';
    case 'document': return '📄';
    case 'code': return '💻';
    case 'archive': return '📦';
    default: return '📁';
  }
}
