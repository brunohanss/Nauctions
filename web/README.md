# Nauctions dashboard

This directory now supplies the shared interface for the Tauri desktop app and browser access. Use `npm run desktop:dev` to run the Windows app or `npm run desktop:build` to create its installer. See [desktop setup](../docs/desktop.md). The desktop uses a native Rust API client; the browser transport below remains available.

Run `npm ci`, then `npm run dev` (or `bun run dev`). Node 22 or later is required for the current test tools. Vite serves the dashboard at `http://127.0.0.1:5173` and proxies `/v1` to the Rust API at `http://127.0.0.1:8080`. If Windows reserves port 5173, use `npm run dev -- --port 4173`. The production server serves the built dashboard from its own origin; use that origin to test authentication.

Run `npm run build` and `npm test` for validation. Serve `dist` from the same origin as the API in production. The dashboard uses the session cookie. It reads the CSRF token from `/v1/me` and includes it in mutations. No credential is stored in browser local storage.

`package-lock.json` is the canonical dependency lock. npm audit reports zero known vulnerabilities for the installed tree. Bun 1.1.31 could not resolve the patched Vitest version; use npm for dependency installation. Bun can run the scripts after installation.

The API sends rational price numerators and denominators as decimal strings. The dashboard uses BigInt for exact formatting, then rounds to the nearest whole copper only at display time (halves away from zero). Chart coordinates and displayed discount percentages are approximate; the API computes ranking and signal eligibility with exact rational comparisons.

The default screen contains no fixture prices. Create a member through the API bootstrap procedure, sign in, pair a collector, and ingest a validated source. Unknown listing depth remains unavailable. Charts show discrete observations. They do not interpolate between scans. Daily imported summaries remain distinct from scan points. All displayed instants use Europe/Paris.

The UI covers market search, server sort and cursor paging, item history, listing depth, opportunities, watchlists, own alert rules, alert evidence, device pairing and revocation, and admin source/member management. API failures appear in an error panel. An empty result does not imply zero market inventory.

The Strategy tab adds exploratory weekly, intraday, and market-dump research. Review a candidate to inspect coverage, a weekly heatmap, and a whole-stack trade calculator. Enter fees and deposit assumptions explicitly. Conditional profit is not a verified sale forecast. See [Strategy rules and limits](../docs/strategy.md).
