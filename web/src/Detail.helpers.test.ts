import { describe, expect, it } from "vitest";
import type { Item } from "./api";
import { canConnectObservations, observedPoints, priceDifference } from "./Detail.helpers";

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
  it("breaks lines at missing values and gaps beyond 45 minutes", () => {
    expect(canConnectObservations(point(0), point(45), "min_buyout")).toBe(true);
    expect(canConnectObservations(point(0), point(46), "min_buyout")).toBe(false);
    expect(canConnectObservations(point(0), point(1, false), "min_buyout")).toBe(false);
    expect(canConnectObservations(point(0), point(1, false), "median_buyout")).toBe(true);
    expect(canConnectObservations(point(0), point(0), "median_buyout")).toBe(false);
  });
});

describe("exact price difference", () => {
  it("subtracts large rational prices without floating-point cancellation", () => {
    expect(priceDifference({ numerator: "9007199254740993", denominator: "1" }, { numerator: "9007199254740994", denominator: "1" })).toBe("−1c");
    expect(priceDifference({ numerator: "3", denominator: "2" }, { numerator: "1", denominator: "1" })).toBe("+0.5c");
    expect(priceDifference(null, { numerator: "1", denominator: "1" })).toBe("—");
  });
});
