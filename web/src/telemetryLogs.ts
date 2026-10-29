import { safeProperties } from './telemetryPrivacy';
// Own the final OTLP payload: SDK log enrichment can add the invitation URL
// after beforeSend. No browser context is available to this payload builder.
export function logPayload(code: string, level: 'info' | 'error', attributes: Record<string, unknown>) {
  return { resourceLogs: [{ resource: { attributes: [{ key: 'service.name', value: { stringValue: 'nauctions-ui' } }] }, scopeLogs: [{ scope: { name: 'nauctions' }, logRecords: [{
    timeUnixNano: (BigInt(Date.now()) * 1000000n).toString(), severityNumber: level === 'error' ? 17 : 9,
    body: { stringValue: code }, attributes: Object.entries(safeProperties(attributes)).map(([key, value]) => ({ key, value: { stringValue: String(value) } })),
  }] }] }] };
}
let pending = 0;
export function sendSafeLog(key: string, code: string, level: 'info' | 'error', attributes: Record<string, unknown>) {
  if (pending >= 4) return;
  pending++;
  void fetch('https://eu.i.posthog.com/i/v1/logs', { method: 'POST', credentials: 'omit', referrerPolicy: 'no-referrer',
    headers: { Authorization: `Bearer ${key}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(logPayload(code, level, attributes)), signal: AbortSignal.timeout(3000),
  }).catch(() => {}).finally(() => pending--);
}
