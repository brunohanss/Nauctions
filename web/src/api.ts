import { desktopInvoke, isDesktop } from './desktop';
export type Price = {
  numerator: number | string;
  denominator: number | string;
};
export type Item = {
  item_id?: number | string | null;
  category?: string | null;
  rarity?: number | null;
  icon?: string | null;
  catalog_attribution?: { text: string; url: string } | null;
  opportunity_score?: number | null;
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
  ) {
    super(message);
  }
}
let csrf = "";
export function setCsrf(token: string) {
  csrf = token;
}
export async function api<T>(
  path: string,
  method = "GET",
  body?: unknown,
): Promise<T> {
  const response = isDesktop() ? await (async () => {
    const reply = await desktopInvoke<{ status: number; body: unknown }>('api_request', { path, method, body: body ?? null, csrf: method === 'GET' ? null : csrf })
      .catch(error => { throw new Error(typeof error === 'string' ? error : 'The desktop request failed.'); });
    return new Response(reply.status === 204 ? null : JSON.stringify(reply.body), { status: reply.status, headers: { 'Content-Type': 'application/json' } });
  })() : await fetch("/v1" + path, {
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
    throw new ApiError(response.status, message);
  }
  return response.status === 204 ? (undefined as T) : response.json();
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
    // Round only at display time, to the nearest 0.01 copper (halves up).
    const cents = (n * 100n + d / 2n) / d,
      gold = cents / 1000000n,
      silver = (cents % 1000000n) / 10000n,
      rest = cents % 10000n;
    const fraction = (rest % 100n)
      .toString()
      .padStart(2, "0")
      .replace(/0+$/, "");
    return `${gold ? `${gold.toLocaleString("en-GB")}g ` : ""}${silver ? `${silver}s ` : ""}${rest / 100n}${fraction ? `.${fraction}` : ""}c`;
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
export function discount(item: Item): number | null {
  const a = item.min_buyout,
    b = item.baseline.price;
  return a && b && Number(a.denominator) > 0 && Number(b.numerator) > 0
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
