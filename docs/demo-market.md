# Synthetic early market

This fixture models the first 63 days of a new auction market. It uses 50 real
Classic item identities, with ten items in each UI category. All asking prices,
supply, shocks, and bargains are invented. It contains no confirmed sales.
It is a controlled test fixture, not an estimate of real Forever player activity.

The isolated market is `EU / Nauctions Demo / Alliance`, UUID
`de000000-0000-4000-8000-000000000001`. Scans use `source_kind=demo`,
`quality=complete`, and `realm_explicit=true`. No player or device is invented.
The app labels this market as simulated and read-only.

## Generate and validate

Run from the repository root in PowerShell:

```powershell
node scripts/demo-market.mjs --anchor 2026-10-28T08:00:00Z
node --test --test-isolation=none scripts/demo-market.test.mjs
cargo build -p market-analysis --bin strategy-inspect
node scripts/verify-demo-strategies.mjs
```

The output is in `artifacts/demo/`. It includes `seed.sql`, `scenarios.json`,
`README.md`, eight `strategy-*.json` files, measured `results-*.json` files,
and `strategy-validation.json`. Generated artifacts are ignored by Git.
The JSON fixture format is defined in [strategy-datasets.md](strategy-datasets.md).

Generation is deterministic. Seed label 42 identifies the fixed version 3 random
streams. Each item has a separate stream derived from its identity. The scan
schedule has its own stream. A changed replay anchor reuses the scan IDs.

## Market and collection model

The generator first creates an hourly latent market, with timestamp jitter of
up to 16 minutes. It then removes scans. Missing observations cannot change the
latent prices or supply. Lower early supply grows as the market develops.
Prices have small correlated fluctuations, with more noise during launch.
Listing quantities, stack sizes, listing spreads, and undercuts also vary.

The app dataset contains 701 complete scans, 35,050 item observations, and
157,636 listings. It stays below the strategy API's 50,000-observation limit.
Each observation agrees with its listing depth and quantity-weighted lower
median price. Prices use exact rational copper per unit.

Collection includes missed hours, two multi-day outages, three-minute bursts,
and recurring player sessions. Recurring sessions preserve positive controls.
This selection is deliberate; it is not evidence that real players will provide
the same coverage. Pattern phases are relative to the replay anchor so the
fixture remains useful at different replay times.

The eight analysis variants are irregular app sampling, dense sampling, random
removal of 50% and 80%, evening-only sampling in Europe/Paris, missing blocks,
five identical copies of each dense scan, and a two-snapshot sparse history.
All variants use the same latent market. Filtering can remove the latest scan;
the harness then reports its actual freshness.

## Scenarios and acceptance

`scenario` preserves the existing chart and bargain examples.
`strategy_scenario` identifies the strategy control. These labels do not enter
the Rust detector.

| Control | Simulated behavior | Required check for irregular sampling |
| --- | --- | --- |
| Stable | Small price fluctuations | No detected strategy |
| Rising | Demand grows as players level | Show rising history; no forced Watch result |
| Falling | Supply grows as players level | No detected strategy |
| Weekly cycle | Low entry period, higher prices over the next few days | Weekly Watch |
| Intraday recovery | Repeated cheap period followed by higher prices within hours | Intraday Watch |
| Failed validation | Early recoveries stop in the final third | No seasonal Watch |
| Supply shock | Supply falls and prices rise near the end | Show price and depth change |
| Market dump | Latest price falls and supply surges | Preliminary dump; no buy instruction |
| Real bargain | One small current stack is underpriced | Ten items clear the 30% opportunity filter |
| False bargain | Old high prices make the chart look cheap | No opportunity against the recent reference |

At the documented anchor, irregular sampling gives 15 detected Watch patterns
and 19 detected Preliminary patterns. The latter include failed validation and
unvalidated rising-price patterns. These are model results, not profitable trades.
Removing 80% of scans reduces Watch patterns to five. Duplicate results must
equal dense results exactly. The two-snapshot variant must be Insufficient.

The final stack floor for non-bargain examples preserves the ten existing
opportunities. It does not alter historical training prices. Current depth and
observations are recalculated together. Stable and failed-validation controls
remain required even when the positive controls pass.

## Load the demo database

Let the server apply database migrations first. Use the existing local database:

```powershell
docker cp artifacts/demo/seed.sql nauctions-dev-postgres-1:/tmp/nauctions-demo.sql
docker exec nauctions-dev-postgres-1 psql -U nauctions -d nauctions -v ON_ERROR_STOP=1 -q -f /tmp/nauctions-demo.sql
./scripts/demo-preview.ps1 -Port 8081
```

The seed uses a transaction and fixed scan IDs. It replaces only listings and
observations for its own demo scans. It preserves shared item metadata and real
markets. Identity conflicts or non-demo scans in the demo market stop the seed.
It creates no members, devices, uploads, alerts, or trade outcomes.

For the Crafting tab, load the additional local examples:

```powershell
docker cp fixtures/demo/crafting.sql nauctions-dev-postgres-1:/tmp/nauctions-demo-crafting.sql
docker exec nauctions-dev-postgres-1 psql -U nauctions -d nauctions -v ON_ERROR_STOP=1 -q -f /tmp/nauctions-demo-crafting.sql
```

These ten recipes cover five professions, positive and negative estimates,
variable output, a service, and missing prices. Four invented merchant buyback
prices let you test the merchant profit filter. All recipe facts are invented.
Names start with `Demo:`. Load this file only into the local development database.
The desktop skips WoW installation and collector setup for the demo market.

The preview script reads the generated replay anchor and existing local secrets.
Use a free port. A running server needs a restart to use a changed replay anchor
or a changed analysis binary. To use an alternate build, pass `-ServerExecutable`.

Run the API checks against the preview:

```powershell
./scripts/verify-demo.ps1 -Origin http://127.0.0.1:8081
```

The checks verify all 50 items, category counts, icons, depth, ten bargain
memberships, fixed replay time, all 701 history points, and strategy results
against the Rust analysis output. Use `range=all` for the full 63-day history.
The 24-hour and seven-day charts contain real fixture gaps; they are no longer
regular half-hour series. Demo observations cannot be uploaded as real scans,
exported to AHDB, or used to send alerts.
