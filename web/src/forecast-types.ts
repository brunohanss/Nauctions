import type { Price } from './api';
export type Estimate = { status: string; reason: string; method: string; horizon_hours: number; target_at: number; price: Price | null; low: Price | null; high: Price | null; change_percent: number | null; direction: string; observations: number; validation_pairs: number; mean_absolute_log_error: number | null; baseline_error: number | null; range_kind: string };
export type RiseFrequency = { horizon_hours: number; status: string; reason: string; percent: number | null; rises: number; comparisons: number; method: string };
export type ForecastReply = { schema_version: number; market_id: string; realm: string; house: string; item_key: string; generated_at: number; expires_at: number; demo: boolean; rolling: Estimate; rolling_12h?: Estimate; rise_frequencies: RiseFrequency[]; current_median: Price | null };
export type ForecastPage = { items: { name: string; forecast: ForecastReply }[]; total_count: number; next_offset: number | null };
export function forecastFresh(data: Pick<ForecastReply, 'generated_at' | 'expires_at'>, now: number) {
  return Number.isFinite(data.generated_at) && Number.isFinite(data.expires_at) && data.generated_at * 1000 <= now + 300_000 && data.expires_at > data.generated_at && data.expires_at - data.generated_at <= 2700 && data.expires_at * 1000 > now;
}
export const percent = (n: number | null) => n === null || !Number.isFinite(n) ? 'Insufficient data' : `${Math.abs(n) < 0.05 || n >= 0 ? '+' : ''}${(Math.abs(n) < 0.05 ? 0 : n).toFixed(1)}%`;
