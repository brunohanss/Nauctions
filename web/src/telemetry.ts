import posthog from 'posthog-js/dist/module.no-external';
import 'posthog-js/dist/exception-autocapture';
import { isDesktop, currentRoute } from './desktop';
import { sendSafeLog } from './telemetryLogs';
import { safeProperties, safeScreen, safeExceptionList } from './telemetryPrivacy';

declare const __POSTHOG_KEY__: string;
declare const __RELEASE__: string;
declare const __REVISION__: string;
export const buildInfo = { release: __RELEASE__, revision: __REVISION__ };
type Properties = Record<string, string | number | boolean | null>;
type Entry = { at: string; code: string; level: 'info' | 'error'; properties: Properties };
const entries: Entry[] = [];
const recent = new Map<string, number>();
let ready = false;
let preference = true;
try { preference = localStorage.getItem('nauctions.telemetry') !== 'off'; } catch { /* Storage can be unavailable. */ }
export const telemetryEnabled = () => preference;
const base = () => ({ ...buildInfo, component: isDesktop() ? 'desktop-ui' : 'browser-ui' });

export function initTelemetry() {
  // Tests and development must not write into the live project.
  if (!import.meta.env.PROD || !__POSTHOG_KEY__) return;
  try {
    posthog.init(__POSTHOG_KEY__, {
      api_host: 'https://eu.i.posthog.com',
      defaults: '2026-08-30',
      autocapture: false,
      capture_pageview: false,
      capture_pageleave: false,
      capture_exceptions: false, // Capture through the privacy boundary below.
      disable_session_recording: true,
      enable_recording_console_log: false,
      disable_external_dependency_loading: true,
      advanced_disable_flags: true,
      person_profiles: 'never',
      persistence: 'localStorage',
      ip: false,
      opt_out_capturing_by_default: !preference,
      request_batching: true,
      before_send: event => {
        if (!event || !(event.event.startsWith('nauctions.') || event.event === '$exception')) return null;
        const original = event.properties ?? {};
        const properties: Record<string, unknown> = { ...safeProperties(original), ...base(), $geoip_disable: true };
        // Retain anonymous installation/session correlation, without URL, referrer or person data.
        for (const key of ['distinct_id', '$device_id', '$session_id', '$window_id', '$lib', '$lib_version', '$os', '$os_version', '$browser', '$browser_version', '$process_person_profile']) {
          if (original[key] !== undefined) properties[key] = original[key];
        }
        if (event.event === '$exception') {
          properties.$exception_list = safeExceptionList(original.$exception_list);
          properties.$exception_fingerprint = `${base().component}:${properties.action ?? 'failure'}:${properties.operation ?? 'unknown'}`;
        }
        event.properties = properties;
        return event;
      },
    });
    ready = true;
    if (preference) posthog.opt_in_capturing({ captureEventName: false });
  } catch { /* Monitoring must never prevent startup. */ }
  window.addEventListener('error', event => reportFailure('unhandled_error', {}, event.error));
  window.addEventListener('unhandledrejection', event => reportFailure('unhandled_rejection', {}, event.reason));
  track('app_opened');
}

export function setTelemetryEnabled(value: boolean) {
  preference = value;
  try { localStorage.setItem('nauctions.telemetry', value ? 'on' : 'off'); } catch { /* Optional persistence. */ }
  if (ready) { try { if (value) posthog.opt_in_capturing({ captureEventName: false }); else posthog.opt_out_capturing(); } catch { /* Optional telemetry. */ } }
}

function append(code: string, level: Entry['level'], properties: Properties) {
  entries.push({ at: new Date().toISOString(), code, level, properties });
  if (entries.length > 100) entries.shift();
}
export const diagnosticsEntries = () => [...entries];
export function track(code: string, input: Properties = {}) {
  const properties = { ...base(), ...safeProperties(input) } as Properties;
  append(code, 'info', properties);
  if (!ready || !preference) return;
  try {
    posthog.capture(`nauctions.${code}`, properties);
    sendSafeLog(__POSTHOG_KEY__, `nauctions.${code}`, 'info', properties);
  } catch { /* Best effort. */ }
}
export function reportFailure(code: string, input: Properties = {}, original?: unknown): string {
  const incident = typeof input.request_id === 'string' ? input.request_id : crypto.randomUUID();
  const properties = { ...base(), ...safeProperties(input), incident_id: incident, action: code, operation: safeProperties(input).operation ?? code } as Properties;
  append(code, 'error', properties);
  const key = `${code}:${input.operation ?? ''}:${input.status ?? ''}`;
  if (Date.now() - (recent.get(key) ?? 0) < 30_000) return incident;
  if (recent.size > 100) recent.clear();
  recent.set(key, Date.now());
  if (!ready || !preference) return incident;
  try {
    const error = new Error(code);
    // Keep frame locations, but discard the original message and source text.
    if (original instanceof Error && original.stack) error.stack = `Error: ${code}\n${original.stack.split('\n').slice(1, 41).join('\n')}`;
    posthog.captureException(error, properties);
    sendSafeLog(__POSTHOG_KEY__, `nauctions.${code}`, 'error', properties);
  } catch { /* Best effort. */ }
  return incident;
}
export const trackScreen = (route = currentRoute()) => track('screen_viewed', { screen: safeScreen(route) });
