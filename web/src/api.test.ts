import { describe, it, expect, vi, afterEach } from "vitest";
import { api, setCsrf, ApiError, price, discount, age, type Item } from "./api";
const native = vi.hoisted(() => ({ isDesktop: vi.fn(() => false), desktopInvoke: vi.fn() }));
vi.mock('./desktop', () => native);
afterEach(() => { vi.unstubAllGlobals(); native.isDesktop.mockReturnValue(false); native.desktopInvoke.mockReset(); });
describe("session requests", () => {
  it("uses native transport for desktop sessions and preserves CSRF", async () => {
    native.isDesktop.mockReturnValue(true);
    native.desktopInvoke.mockResolvedValue({ status: 200, body: { ok: true } });
    const fetchMock = vi.fn(); vi.stubGlobal('fetch', fetchMock);
    setCsrf('native-csrf');
    expect(await api('/watchlists', 'POST', { item_key: 'i123' })).toEqual({ ok: true });
    expect(native.desktopInvoke).toHaveBeenCalledWith('api_request', { path: '/watchlists', method: 'POST', body: { item_key: 'i123' }, csrf: 'native-csrf' });
    expect(fetchMock).not.toHaveBeenCalled();
  });
  it("preserves native unauthorized and empty response behavior", async () => {
    native.isDesktop.mockReturnValue(true);
    native.desktopInvoke.mockResolvedValueOnce({ status: 401, body: { error: 'login_required' } });
    await expect(api('/me')).rejects.toEqual(new ApiError(401, 'login_required'));
    native.desktopInvoke.mockResolvedValueOnce({ status: 204, body: null });
    await expect(api('/auth/logout', 'POST', {})).resolves.toBeUndefined();
  });
  it("turns native connection failures into visible error messages", async () => {
    native.isDesktop.mockReturnValue(true);
    native.desktopInvoke.mockRejectedValue('Cannot reach the server.');
    await expect(api('/me')).rejects.toThrow('Cannot reach the server.');
  });
  it("uses same-origin cookies and the server-issued CSRF token for mutation", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    setCsrf("test-csrf");
    await api("/watchlists", "POST", { item_key: "123:variant" });
    expect(fetchMock).toHaveBeenCalledWith(
      "/v1/watchlists",
      expect.objectContaining({
        credentials: "same-origin",
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "X-CSRF-Token": "test-csrf",
        },
        body: '{"item_key":"123:variant"}',
      }),
    );
  });
  it("rejects unauthorized requests without treating them as empty results", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ message: "Session expired" }), {
          status: 401,
        }),
      ),
    );
    await expect(api("/items")).rejects.toEqual(
      new ApiError(401, "Session expired"),
    );
  });
});
describe("observation presentation", () => {
  it("preserves prices above the JavaScript safe integer range", () => {
    expect(price({ numerator: "9007199254740993", denominator: "1" })).toBe(
      "900,719,925,474g 9s 93c",
    );
    expect(price({ numerator: 9007199254740992, denominator: 1 })).toBe(
      "Price outside exact display range",
    );
  });
  it("does not invent a price for missing buyouts", () => {
    expect(price(null)).toBe("No buyout data");
    expect(price({ numerator: 1, denominator: 0 })).toBe("No buyout data");
  });
  it("displays per-unit copper without rounding to whole copper", () => {
    expect(price({ numerator: 20000, denominator: 20 })).toBe("10s 0c");
    expect(price({ numerator: 1, denominator: 2 })).toBe("0.5c");
  });
  it("does not produce a discount for zero or absent references", () => {
    expect(
      discount({
        min_buyout: { numerator: 20, denominator: 1 },
        baseline: { price: null },
      } as Item),
    ).toBeNull();
    expect(
      discount({
        min_buyout: { numerator: 20, denominator: 1 },
        baseline: { price: { numerator: 0, denominator: 1 } },
      } as Item),
    ).toBeNull();
  });
  it("compares fractional unit prices before display rounding", () => {
    expect(
      discount({
        min_buyout: { numerator: 1, denominator: 2 },
        baseline: { price: { numerator: 1, denominator: 1 } },
      } as Item),
    ).toBe(50);
  });
  it("labels elapsed observation age", () => {
    expect(age(14400)).toBe("4 hours");
    expect(age(15)).toBe("Under 1 minute");
  });
});
