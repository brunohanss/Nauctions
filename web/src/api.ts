import { moneyText } from './money';
import { desktopInvoke, isDesktop } from './desktop';
import { reportFailure, track } from './telemetry';
import { safeOperation } from './telemetryPrivacy';
export type Price = {
  numerator: number | string;
  denominator: number | string;
};
export type Item = {
    merchant_unit_price?: Price | null;
    merchant_status?: string;
  item_id?: number | string | null;
  category?: string | null;
  rarity?: number | null;
  icon?: string | null;
  catalog_attribution?: { text: string; url: string } | null;
  opportunity_score?: number | null;
  reference_profit?: Price | null;
  discount_percentage?: number | null;
  freshness_status?: string;
  trend?: { observed_at: string; median_buyout: Price | null; quality: string }[];
  change_24h_percentage?: number | null;
  item_key: string;
  name: string;
  market_id: string;
  observed_at: string;
  received_at: string;
  source_kind: string;
  quality: string;
  sample_count: number;
  min_buyout: Price | null;
  median_buyout: Price | null;
  total_units: number | null;
  listing_count: number | null;
  presence?: "observed" | "absent_in_complete_scan";
  baseline: { price: Price | null; day_count: number; source_count: number };
  freshness_seconds: number;
};
export type Market = {
  demo?: boolean;
  as_of?: string | null;
  read_only?: boolean;
  id: string;
  realm: string;
  house: string;
  last_observed_at: string | null;
  scan_count: number;
  source_count: number;
};
export type Member = {
  id: string;
  display_name: string;
  role: string;
  csrf_token: string;
};
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public requestId: string | null = null,
  ) {
    super(message);
  }
}
let csrf = "";
export function setCsrf(token: string) {
  csrf = token;
}
async function request<T>(
  path: string,
  method = "GET",
  body?: unknown,
): Promise<T> {
  const response = isDesktop() ? await (async () => {
    const reply = await desktopInvoke<{ status: number; body: unknown; request_id?: string | null }>('api_request', { path, method, body: body ?? null, csrf: method === 'GET' ? null : csrf })
      .catch(error => { throw new Error(typeof error === 'string' ? error : 'The desktop request failed.'); });
    return new Response(reply.status === 204 ? null : JSON.stringify(reply.body), { status: reply.status, headers: { 'Content-Type': 'application/json', ...(reply.request_id ? { 'X-Request-ID': reply.request_id } : {}) } });
  })() : await fetch("/v1" + path, {
    signal: AbortSignal.timeout(15_000),
    method,
    credentials: "same-origin",
    headers: {
      ...(body !== undefined ? { "Content-Type": "application/json" } : {}),
      ...(method !== "GET" ? { "X-CSRF-Token": csrf } : {}),
    },
    ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
  });
  if (!response.ok) {
    let message = `Request failed (${response.status}).`;
    try {
      const error = await response.json();
      message = error.message ?? error.error ?? message;
    } catch {}
    throw new ApiError(response.status, message, response.headers.get('X-Request-ID'));
  }
  return response.status === 204 ? (undefined as T) : response.json();
}
export async function api<T>(path: string, method = 'GET', body?: unknown): Promise<T> {
  const start = performance.now(), operation = safeOperation(path);
  try {
    const result = await request<T>(path, method, body);
    if (method !== 'GET') track('api_action_completed', { operation, action: method, duration_ms: Math.round(performance.now() - start) });
    if (method === 'GET' && operation.endsWith('/quote')) track('crafting_quote_viewed');
    return result;
  } catch (error) {
    if (!(error instanceof ApiError) || error.status >= 500) {
      reportFailure('api_request_failed', { operation, status: error instanceof ApiError ? error.status : 0, request_id: error instanceof ApiError ? error.requestId : null }, error);
    }
    if (error instanceof Error && (error.name === 'TimeoutError' || error.name === 'AbortError')) {
      throw new ApiError(408, 'The request timed out. Try again.');
    }
    throw error;
  }
}
export function price(value: Price | null | undefined): string {
  if (!value) return "No buyout data";
  try {
    if (
      (typeof value.numerator === "number" &&
        !Number.isSafeInteger(value.numerator)) ||
      (typeof value.denominator === "number" &&
        !Number.isSafeInteger(value.denominator))
    )
      return "Price outside exact display range";
    const n = BigInt(value.numerator),
      d = BigInt(value.denominator);
    if (n < 0n || d <= 0n) return "No buyout data";
    return moneyText(value);
  } catch {
    return "No buyout data";
  }
}
export function age(seconds: number): string {
  return seconds < 60
    ? "Under 1 minute"
    : seconds < 3600
      ? `${Math.floor(seconds / 60)} minutes`
      : seconds < 86400
        ? `${Math.floor(seconds / 3600)} hours`
        : `${Math.floor(seconds / 86400)} days`;
}
export function discount(item: Item, reference: Price | null = item.baseline.price): number | null {
  const a = item.min_buyout,
    b = reference;
  return a && b && Number(a.denominator) > 0 && Number(b.denominator) > 0 && Number(b.numerator) > 0
    ? 100 *
        (1 -
          Number(a.numerator) /
            Number(a.denominator) /
            (Number(b.numerator) / Number(b.denominator)))
    : null;
}
export function time(value: string | null | undefined) {
  return value
    ? new Date(value).toLocaleString("en-GB", {
        timeZone: "Europe/Paris",
        dateStyle: "medium",
        timeStyle: "short",
      })
    : "No observation";
}
