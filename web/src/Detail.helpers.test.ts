import { describe, expect, it } from "vitest";
import type { Item } from "./api";
import { canConnectObservations, observedPoints, priceDifference, chartProjection } from "./Detail.helpers";

const point = (minute: number, min = true): Item => ({
  observed_at: new Date(Date.UTC(2026, 9, 27, 10, minute)).toISOString(),
  min_buyout: min ? { numerator: "400", denominator: "1" } : null,
  median_buyout: { numerator: "600", denominator: "1" },
  total_units: null,
} as Item);

describe("observed chart data", () => {
  it("sorts existing points without filling gaps or changing unknown quantities", () => {
    const first = point(0), last = point(120);
    const input = [last, first, { ...point(2), observed_at: "invalid" }];
    expect(observedPoints(input)).toEqual([first, last]);
    expect(input[0]).toBe(last);
    expect(observedPoints(input)[0].total_units).toBeNull();
  });
  it("connects recorded prices across time gaps without using missing values", () => {
    expect(canConnectObservations(point(0), point(45), "min_buyout")).toBe(true);
    expect(canConnectObservations(point(0), point(46), "min_buyout")).toBe(true);
    expect(canConnectObservations(point(0), point(1440), "min_buyout")).toBe(true);
    expect(canConnectObservations(point(0), point(1, false), "min_buyout")).toBe(false);
    expect(canConnectObservations(point(0), point(1, false), "median_buyout")).toBe(true);
    expect(canConnectObservations(point(0), point(0), "median_buyout")).toBe(false);
  });
});

describe("exact price difference", () => {
  it("subtracts large rational prices without floating-point cancellation", () => {
    expect(priceDifference({ numerator: "9007199254740993", denominator: "1" }, { numerator: "9007199254740994", denominator: "1" })).toBe("−1c");
    expect(priceDifference({ numerator: "3", denominator: "2" }, { numerator: "1", denominator: "1" })).toBe("+1c");
    expect(priceDifference(null, { numerator: "1", denominator: "1" })).toBe("—");
  });
});


describe("forecast chart endpoints", () => {
  const now = Date.UTC(2026, 9, 27, 10);
  const item = { ...point(0), freshness_status: "fresh" } as Item;
  const forecast = { generated_at: now / 1000, expires_at: now / 1000 + 2700, current_median: item.median_buyout, rolling: { status: "experimental", target_at: now / 1000 + 86400, price: { numerator: "660", denominator: "1" }, low: null, high: null } } as import('./forecast-types').ForecastReply;
  it("uses the actual observation and target timestamps without filling observations", () => {
    expect(chartProjection(item, forecast, now)).toEqual({ anchorAt: now, targetAt: now + 86400000, anchor: 600, target: 660, low: null, high: null });
  });
  it("suppresses expired, stale, mismatched, and invalid targets", () => {
    expect(chartProjection(item, forecast, now + 2700000)).toBeNull();
    expect(chartProjection({ ...item, freshness_status: "stale" }, forecast, now)).toBeNull();
    expect(chartProjection(item, { ...forecast, current_median: { numerator: "601", denominator: "1" } }, now)).toBeNull();
    expect(chartProjection(item, { ...forecast, rolling: { ...forecast.rolling, target_at: now / 1000 } }, now)).toBeNull();
  });
  it("does not invent a range when only one bound or inverted bounds exist", () => {
    const withLow = { ...forecast, rolling: { ...forecast.rolling, low: { numerator: "500", denominator: "1" } } };
    expect(chartProjection(item, withLow, now)?.low).toBeNull();
    expect(chartProjection(item, { ...withLow, rolling: { ...withLow.rolling, high: { numerator: "700", denominator: "1" } } }, now)?.high).toBe(700);
  });
});
