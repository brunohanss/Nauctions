const allowed = new Set(['release', 'revision', 'component', 'screen', 'action', 'outcome', 'operation', 'status', 'duration_ms', 'incident_id', 'request_id', 'queued', 'rejected']);
export function safeProperties(input: Record<string, unknown>): Record<string, string | number | boolean> {
  return Object.fromEntries(Object.entries(input).filter(([key, value]) => allowed.has(key) && (
    typeof value === 'boolean' || (typeof value === 'number' && Number.isFinite(value)) ||
    (typeof value === 'string' && value.length <= 100 && /^[a-zA-Z0-9_./:-]+$/.test(value))
  ))) as Record<string, string | number | boolean>;
}
export function safeScreen(route: string): string {
  if (route.startsWith('/items/')) return 'item_detail';
  return ['market', 'strategy', 'crafting', 'watchlist', 'settings'].find(p => route === `/${p}`) ?? 'other';
}
export function safeOperation(path: string): string {
  const segments = path.split('?')[0].split('/').filter(Boolean);
  const allowed = ['auth', 'join', 'login', 'logout', 'accept-invite', 'me', 'markets', 'items', 'history', 'depth', 'forecast', 'forecasts', 'addon', 'opportunities', 'strategies', 'professions', 'recipe-items', 'recipes', 'quote', 'watchlists', 'alert-rules', 'alerts', 'devices', 'pair-code', 'admin', 'invites', 'members', 'sources'];
  return segments.map(s => allowed.includes(s) ? s : 'id').slice(0, 5).join('/');
}
export function safeExceptionList(input: unknown): unknown[] {
  if (!Array.isArray(input)) return [];
  return input.slice(0, 3).map(exception => ({
    type: 'NauctionsError', value: 'Application operation failed. See operation and incident_id.',
    mechanism: { type: 'generic', handled: exception?.mechanism?.handled !== false },
    stacktrace: { type: 'raw', frames: Array.isArray(exception?.stacktrace?.frames) ? exception.stacktrace.frames.slice(0, 40).map((frame: Record<string, unknown>) => {
      let filename = '';
      const original = String(frame.filename ?? '');
      try {
        if (/^[a-z]:[\\/]/i.test(original)) throw new Error('local path');
        const url = new URL(original);
        if (url.protocol === 'file:') throw new Error('local path');
        filename = url.pathname;
      } catch { filename = original.split(/[?#]/)[0].split(/[\\/]/).pop() ?? ''; }
      const chunk = typeof frame.chunk_id === 'string' && /^[\da-f-]{36}$/i.test(frame.chunk_id) ? frame.chunk_id : undefined;
      return { filename, lineno: frame.lineno, colno: frame.colno, chunk_id: chunk, function: typeof frame.function === 'string' && /^[\w.$<> -]{0,150}$/.test(frame.function) ? frame.function : undefined, in_app: true };
    }) : [] },
  }));
}
