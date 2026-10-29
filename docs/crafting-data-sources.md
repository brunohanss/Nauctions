# Crafting dataset source investigation

Research date: 29 October 2026.

Implementation decision: use the single Nauctions AHDB fork to collect shared
discovered recipes. External catalogue imports and player recipe tracking are
out of scope. See [the current implementation](crafting.md). The source options
below are historical research; they are not active runtime dependencies.

Status: source research only. A LibProfessionDB archive was downloaded and
inspected. No recipes were imported into the database.
No application code, database schema, or game compatibility setting was changed.

## Recommendation

Use **LibProfessionDB as the available offline starting dataset**, subject to
output quantity and build validation. ForeverDB access is pending developer
approval. The user excluded AHledger after reporting a phishing attempt. Do not
contact AHledger or use it for further downloads. Its earlier research below is
historical evidence only. Keep client extraction as the fallback.

This is a provisional recommendation. No source passed the complete-dataset
acceptance check in this investigation.

## Free offline option: LibProfessionDB

Follow-up research found [LibProfessionDB 1.9.1](https://www.curseforge.com/wow/addons/libprofessiondb/files/9095167).
The public CurseForge CDN supplied the ZIP without an API key. The archive is
saved at `artifacts/ProfessionDB-v1.9.1.zip`. Its checksum and inspection counts
are in `artifacts/ProfessionDB-v1.9.1.manifest.json`. No addon code was executed.

The actual `Data/Forever/_core` files identify build **1.60.1.69977**. Static
inspection of the ten `LoadCore` sections found **2,511 profession entries** and
**2,510 distinct spell IDs**:

| Profession | Entries |
|---|---:|
| Alchemy | 195 |
| Blacksmithing | 512 |
| Cooking | 133 |
| Enchanting | 261 |
| Engineering | 275 |
| First Aid | 33 |
| Fishing | 4 |
| Leatherworking | 603 |
| Mining | 18 |
| Tailoring | 477 |

Recipe records include numeric spell keys, `craftedItemId` where present,
`reagents` keyed by item ID with quantities, difficulty, and optional skill and
specialization requirements. Locale files provide names. The ZIP contains an
MIT license with copyright 2026 Pimptasty; preserve its notice.

Important gaps:

- No explicit output quantity fields were found in the ten core files. Do not
  assume that each craft produces one unit. Bronze smelting and batch ammunition
  need reliable yield data before profitability calculations can use them.
- The Forever TOC explicitly flags Vanilla borrowing for some skill requirements,
  sources, and hidden-recipe values. Keep those flags and treat borrowed facts as
  unconfirmed for Forever.
- The source build is older than the repository target. Later recipes and changes
  can be missing. Full parsing, references, and game availability remain unchecked.
- No verified public GitHub mirror of this library was found. This is a free
  downloadable addon dataset, rather than a verified GitHub dataset.

GitHub alternatives were also checked. [Everloot](https://github.com/Everloot/everloot)
publishes open collector code and labels data CC BY-SA 4.0, but no complete static
recipe dump was verified. [WoW Classic Item Caches](https://github.com/Napalmsteak/WoW-Classic-Item-Caches)
explicitly says its Forever snapshot excludes recipe lists; it cannot replace
this crafting catalog. [Imemoth/wow-foreverdb](https://github.com/Imemoth/wow-foreverdb)
is an observed-loot collector with recipe probes, not a verified complete recipe
snapshot. It is a separate project from ForeverDB.net.

## Repository findings

- The backend uses Rust, Axum, Tokio, and PostgreSQL through SQLx. There is no ORM.
- `db/migrations/0001_core.sql` defines items, markets, listings, observations,
  daily metrics, jobs, alerts, and deliveries. Later migrations extend this schema.
- `items.item_key` is the canonical AHDB identity. `base_item_id` stores the game
  item ID. A crafting import must preserve the difference between base IDs and
  item variants. Do not match recipes to item names.
- `crates/ahdb-format` parses AHDB. `crates/server/src/ingest.rs` and `worker.rs`
  process uploads. Individual listings and aggregate observations already exist.
- Markets use region, realm, and auction house. Recipe records can be shared;
  all price calculations must use one explicit market ID.
- `crates/domain` provides exact rational unit prices. `crates/analysis` provides
  market analysis. Reuse these components for crafting costs.
- Existing routes include `/v1/opportunities`, `/v1/strategies`, item history,
  and market depth. The interface uses React and a Windows Tauri shell.
- The worker runs on a Tokio interval. The database already has a jobs table.
- Checks include Cargo tests, Clippy, formatting, web tests/build, and
  `scripts/integration-db.ps1`. Database tests need PostgreSQL.
- The repository target remains `1.60.1.70630`. A source with a different build
  is not proof of compatibility. The local icon catalog is display metadata,
  not a crafting catalog.

## Source comparison

| Check | AHledger | ForeverDB | Client data / Wago |
|---|---|---|---|
| Forever data | Claimed; public recipe inspected | Claimed; public profession inspected | Must verify exact product and build |
| Complete export | Not verified | Pagination documented; export not verified | Extraction not performed |
| Recipe and item IDs | Export example; public links | API contract needs payload review | Table relationships need decoding |
| Reagent quantities | Public recipe inspected | Public recipe list inspected | SpellReagents candidate |
| Output quantities | Example and public recipe | Must inspect API payload | Spell effects need interpretation |
| New/modified flags | Full export coverage unknown | Documented | Needs same-build baseline comparison |
| Build provenance | Site reports 70658 | API metadata documented | Exact client manifest required |
| Access | No API key documented | Approved API key required | Wago access failed |
| Rights | Attribution and feed restrictions | Attribution and resale restrictions | Tool license is not a game-data license |

### A. AHledger

The [data page](https://ahledger.com/data) reports client build 1.60.1.70658,
imported on 28 October. It links to `/api/catalog?class=trade-goods`. That link
does not establish a full recipe export.

The [API documentation](https://ahledger.com/developers) describes
`/v1/crafting/{market}?profession=Tailoring&all=1`, with JSON or CSV.
It shows spell IDs, output IDs and quantity ranges, skill, and reagents.
Access is documented without a key, at 300 requests per minute. Honor cache
headers. The example contains a limit of 500; pagination and truncation need
verification. `all=1` includes unpriced recipes, but does not prove coverage of
enchants, gathering, or non-item effects.

The [recipe index](https://ahledger.com/wow-forever/recipes) displays 2,406
recipes across 11 profession groups. These are site counts, not imported counts.

One [actual public recipe](https://ahledger.com/wow-forever/recipes/1245250-elixir-of-minor-force)
identifies spell 1245250, output item 247755, output quantity 1, skill 1,
and reagents 3371 and 765, one of each. IDs were checked through its item links.
This verifies one displayed relationship, not the export.

Terms require visible linked attribution and prohibit resale as a feed or
claiming the data as our own scans. Keep those restrictions with archived data.

### B. ForeverDB

The [developer documentation](https://foreverdb.net/developers) describes:

- `/api/v1/meta` for build and counts.
- `/api/v1/professions` for twelve professions and recipe counts.
- `/api/v1/professions/{slug}?limit=100&offset=0` for paginated recipes.
- `/api/v1/poisons` for a separate class-skill dataset.

The contract includes reagents, results, skill colours, acquisition, and Forever
changes. Responses include `data` and `meta`, with totals and next-page details.
Exact identifier fields, output ranges, probabilities, and tools need payload
verification.

Access needs a Battle.net-linked account and developer approval. Send the key
through `X-API-Key` or Bearer authentication. Limits are 60 requests per minute
and 10,000 per day; a limit response uses 429 and `Retry-After`.

Credit ForeverDB visibly. Do not resell or claim ownership. Recipes are described
as client data; Classic and player-observed acquisition data are labelled.
Confirm that the intended archive and deployment comply with these terms.

The [public Alchemy page](https://foreverdb.net/profession/alchemy) displays
195 recipes, 71 new and 11 changed. Its first page exposes 50 entries and a
next-page link. This supports the presence of real recipe content, but does not
verify API pagination or full coverage.

### C. Wago and direct client extraction

[Wago](https://wago.tools) returned HTTP 403 through the web tool. No Forever
build or CSV export was verified. Do not substitute Retail or Classic Era data.

[wow.tools.local](https://github.com/Marlamin/wow.tools.local) can load a selected
local WoW folder and product. Its documentation also covers local dependencies
and hotfix storage. This is an extraction tool, not a ready recipe dataset.

[WoWDBDefs](https://github.com/wowdev/WoWDBDefs) supplies table definitions.
Candidate joins are SkillLine/SkillLineAbility to spells, SpellReagents to input
items, SpellEffect to created items, and item tables to item metadata. Definitions
expose reagent counts, skill ranks, effect item types, and effect indexes.
This is an extraction hypothesis. Exact layouts and effect semantics must be
verified against the selected Forever build. Triggered spells, random outputs,
hotfixes, and server-held data can require additional handling.

The tools' open-source licenses do not grant redistribution rights for Blizzard
game data. No game-data redistribution permission was established here.

## Other candidates and discrepancies

[Endgame Tools](https://endgametools.com/en/wow-forever/professions) reports build
1.60.1.70685 and includes Poisons. [The Forever Era](https://theforeverera.com/en/recipes/)
displays 2,208 recipes and 13 professions, including Comprehension. Neither was
verified as a downloadable dataset with suitable reuse terms.

Counts alone cannot select the best source. A source can include unused spells,
class skills, enchants, or old records that another source excludes. For example,
their displayed Minor Frenzy Potion requirements differ: AHledger shows 5 and
ForeverDB shows 40. Preserve build provenance and inspect
the client record before resolving such a conflict.

## Access results

The web tool could read public documentation and recipe pages. The AHledger
catalog request returned a cache miss. ForeverDB API and Wago table requests
did not return readable payloads through that tool.

PowerShell `Invoke-WebRequest` requests to the AHledger same-host catalog,
ForeverDB Alchemy API, and Wago CSV candidate each failed with an unexpected
connection-close error. No HTTP status, JSON, CSV, or rate-limit headers were
obtained. This does not prove an authentication failure or endpoint absence.

`docs/external-data.md` records an earlier Bitdefender block on direct AHledger
API access. No security exception was added. No request to `api.ahledger.com`
was made from the local shell during this investigation.

## Next verification steps

1. Obtain an approved ForeverDB key or a source-provided export. Keep credentials
   outside version control.
2. Archive the raw metadata, profession index, and every recipe page. Record URLs,
   retrieval times, build, response counts, and SHA-256 hashes.
3. Fetch Poisons separately. Check whether other class skills or Comprehension
   belong in the selected build's catalog.
4. Check every required field against actual responses. Keep non-item crafts and
   unknown outputs explicit. Do not infer probabilities or zero quantities.
5. Check stable totals, unique recipe identities, terminal pagination, and one
   consistent build. Repeat metadata retrieval after export. Reject a mixed or
   incomplete snapshot for recipe deactivation.
6. Compare representative new and modified recipes with the exact client build.
   Resolve discrepancies before calling coverage complete.
7. Only then implement the local importer in the existing Rust/PostgreSQL stack.
   Calculations must read the local catalog and existing market observations.

No runtime API dependency is required by this proposed workflow. No importer,
migration, profitability feature, or benchmark is claimed as implemented.
Tests were not run because this change only records source research.
