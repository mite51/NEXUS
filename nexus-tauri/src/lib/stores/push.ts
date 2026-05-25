import { writable, derived } from 'svelte/store';

export interface PushSession {
  session_id: string;
  sender_did: string;
  filename: string;
  shards_received: number;
  shards_total: number;
  status: 'accepted' | 'progress' | 'complete' | 'stored' | 'denied' | 'failed';
  reason?: string;
  started_at: number;
  updated_at: number;
}

// All tracked push sessions (active + recent)
export const pushSessions = writable<Map<string, PushSession>>(new Map());

// Active sessions (not yet stored/failed/denied)
export const activePushSessions = derived(pushSessions, $sessions => {
  return [...$sessions.values()].filter(
    s => s.status === 'accepted' || s.status === 'progress' || s.status === 'complete'
  );
});

// Recently completed sessions (last 5 minutes)
export const recentPushSessions = derived(pushSessions, $sessions => {
  const fiveMinAgo = Date.now() - 5 * 60 * 1000;
  return [...$sessions.values()]
    .filter(s => (s.status === 'stored' || s.status === 'failed' || s.status === 'denied') && s.updated_at > fiveMinAgo)
    .sort((a, b) => b.updated_at - a.updated_at);
});

// Count of active incoming pushes (for sidebar badge)
export const activePushCount = derived(activePushSessions, $active => $active.length);

/**
 * Update push session state from a nexus://push-progress event payload
 */
export function updatePushSession(payload: {
  session_id: string;
  sender_did: string;
  filename: string;
  shards_received: number;
  shards_total: number;
  status: string;
  reason?: string;
}) {
  pushSessions.update(map => {
    const existing = map.get(payload.session_id);
    const now = Date.now();

    map.set(payload.session_id, {
      session_id: payload.session_id,
      sender_did: payload.sender_did || existing?.sender_did || '',
      filename: payload.filename || existing?.filename || '',
      shards_received: payload.shards_received,
      shards_total: payload.shards_total,
      status: payload.status as PushSession['status'],
      reason: payload.reason,
      started_at: existing?.started_at || now,
      updated_at: now,
    });

    return map;
  });
}

/**
 * Clean up old completed sessions (older than 10 minutes)
 */
export function cleanupOldSessions() {
  const tenMinAgo = Date.now() - 10 * 60 * 1000;
  pushSessions.update(map => {
    for (const [id, session] of map) {
      if (
        (session.status === 'stored' || session.status === 'failed' || session.status === 'denied') &&
        session.updated_at < tenMinAgo
      ) {
        map.delete(id);
      }
    }
    return map;
  });
}
