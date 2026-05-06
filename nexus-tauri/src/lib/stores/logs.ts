import { writable, derived } from 'svelte/store';

export type LogLevel = 'info' | 'warn' | 'error' | 'success';

export interface LogEntry {
  id: number;
  timestamp: number;
  level: LogLevel;
  source: string;
  message: string;
  detail?: string;
}

let nextId = 0;

export const logs = writable<LogEntry[]>([]);
export const unreadLogCount = writable<number>(0);

export function addLog(level: LogLevel, source: string, message: string, detail?: string) {
  const entry: LogEntry = {
    id: nextId++,
    timestamp: Date.now(),
    level,
    source,
    message,
    detail,
  };
  logs.update(list => {
    const updated = [entry, ...list];
    // Keep last 500 entries
    return updated.slice(0, 500);
  });
  unreadLogCount.update(n => n + 1);
}

export function clearLogs() {
  logs.set([]);
}

export function markLogsRead() {
  unreadLogCount.set(0);
}
