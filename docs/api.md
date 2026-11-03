# API contract

All routes use `/v1`. Browser requests use an HttpOnly SameSite=Strict cookie. HTTPS cookies also use Secure. Mutations require an exact `Origin` and `X-CSRF-Token` from `GET /v1/me`. Device tokens cannot read member data.

## Authentication

- `POST /auth/login`: `{login,password}`. Sets the browser session. Password length is 12 to 256 characters.
- `POST /auth/logout`: ends the current session.
- `POST /auth/accept-invite`: `{code,login,display_name,password}`. The client must then log in.
- `GET /me`: identity, role, and CSRF token.
- `POST /admin/invites`: `{role}`. Returns a one-use 24-hour code.
- `GET /admin/members`, `PATCH /admin/members/{id}`: administration. Status is `active` or `disabled`.

## Collector

- `GET /collector/capabilities`: public non-sensitive version and size policy, used before pairing.
- `GET /collector/history?realm=NAME&house=Alliance`: active device token required. Returns a version-1 `HistoryExport` with exact realm/house, generation time, recent observations, and UTC daily summaries. Horde and Neutral are also valid configured houses. The requested scope must match the server market. The maximum export is 100,000 observations/summaries and 32 MiB; excessive data is rejected without truncation. Integer values use decimal strings except schema version and Unix timestamps. Export includes base-item history only; variant prices are not collapsed. Legacy history without explicit realm evidence is excluded. This endpoint grants no access to member data or credentials.
- `POST /devices/pair-code`: member session required. Returns a one-use 10-minute pairing code.
- `POST /devices/enroll`: `{code,display_name}`. Returns `device_id` and the device token once.
- `GET /devices`, `DELETE /devices/{id}`: view or revoke owned devices; admins can revoke other devices.
- `POST /uploads`: Bearer device token, raw uncompressed bytes. Required headers: `X-Upload-ID` UUID, `X-Content-SHA256`, and `X-Source-File-ID` 64-character hash. Maximum body: 32 MiB.
- `GET /uploads/{upload_id}`: returns `{upload_id,status,error_code}` for the authenticated device only.
- `POST /collector/health`: accepts bounded non-identifying health fields.

`202` means durable encrypted staging, not successful parsing. Status is `received`, `accepted`, or `rejected`. Reuse of an ID with different bytes returns `409`. Keep queued bytes until an accepted receipt. Authentication failures stop upload attempts; transient errors use backoff. Rejected bytes stay visible locally.

## Market reads

- `GET /markets`: configured realm/house, observation age, and counts.
- `GET /items`: `market_id`, `q`, `sort=name|price|discount|freshness`, `freshness=all|fresh|stale`, `limit=1..100`, and opaque `cursor`.
- `GET /items/{key}`: latest observation and baseline.
- `GET /items/{key}/history`: `market_id`, `range=24h|7d|30d`. Discrete points, yesterday reference range, and distinct UTC daily imports.
- `GET /items/{key}/depth`: latest eligible full-scan listings. Missing detail is unavailable, not zero inventory.
- `GET /opportunities`: qualified current observations with the same list filters.

Use `category=watchlist` on `/items` or `/opportunities` to return only the signed-in member's saved items. The server applies this filter before sorting and pagination. Search and market scope still apply.

If an older server rejects this category, the Market client reads the sorted `category=all` pages and filters them against the member's watchlist before display pagination. This fallback retains search and market scope and refuses results beyond the 2,000-item read limit.

Encode item keys as URL path components. Never collapse suffix keys into base item histories. Prices have `{numerator,denominator}` in copper per unit. Both fields are decimal strings. Timestamps use UTC ISO 8601. Missing prices are null. Query limits reject overly broad requests instead of returning a silently incomplete ranking.

## Member preferences

- `GET/POST /watchlists`, `DELETE /watchlists/{key}`.
- `GET/POST /alert-rules`, `PATCH/DELETE /alert-rules/{id}`.
- Rule fields: `item_key`, `kind=underpriced|threshold|movement`, `threshold_copper`, `discount_percent`, `enabled`, `notify`.
- `GET /alerts`: the member's most recent 200 events and delivery states.
- `GET /admin/sources`: sanitized source diagnostics.

Rule deletion retires the rule and keeps its event evidence. Alert sends are opt-in. The default suppression interval is one hour. A price improvement of at least 10 percent can bypass suppression. Replaying the same scan and rule revision cannot add another event.

Errors return `{error,code}` with a machine-readable code. Database failures do not expose SQL, credentials, source contents, or filesystem paths.
# Nickname onboarding

`POST /v1/auth/join` accepts `{"nickname":"Example","code":"COPAINS"}`. The configured Origin is required. The endpoint is rate limited. A successful response sets a persistent HttpOnly, SameSite=Strict member cookie and returns the CSRF token. HTTPS responses also set Secure. The common invitation grants only the member role. Nicknames are case-insensitively reserved; this endpoint never retrieves an existing identity from a nickname and the common code. Invalid codes return 403, invalid names 400, and reserved names 409.

Migration 0004 adds a persistent-session flag. Existing password sessions remain short lived. Remembered memberships have a one-year validity and renew during active use when less than six months remain. Existing member deactivation removes these sessions. The Tauri client protects remembered credentials with Windows DPAPI and restores them only for the exact configured server origin. Administrator password login and one-time administration invitations remain available through the existing endpoints.

