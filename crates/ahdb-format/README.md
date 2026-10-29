# AHDB data parser

This crate reads data. It does not run Lua code.

Use `cargo run -p ahdb-format --bin ahdb-inspect -- --input fixtures/synthetic-v6.lua` to print normalized JSON. The command reads the input file. It does not change the file.

Supported source schema:

- Full records: `Record.lua` data format 6.
- Item metadata: `itemDB_2` format 5.
- History: `houses[house].items` with four comma-separated fields.
- Daily history: `houses[house].daily` with five comma-separated fields. Days are UTC days.
- Preferred global: `AuctionDBForeverSaved`. Explicit legacy global names are accepted only when their data has the supported schema.

Source references, read on 27 October 2026:

- https://github.com/mooreatv/AuctionDB/blob/master/AuctionDB/Record.lua
- https://github.com/mooreatv/AuctionDB/blob/master/AuctionDB/History.lua
- https://github.com/mooreatv/AuctionDB/blob/master/AuctionDB/AuctionDB.lua

The implementation follows the schema. It does not copy addon code. The source branch can change. Before production use, pin the addon release and capture a consented, sanitized client file.

## Safety limits

Default limits are 32 MiB per file, 24 MiB per string, 48 table levels, 1,000,000 syntax nodes, and 500,000 decoded records across scans and history. Integer data must fit a nonnegative PostgreSQL BIGINT. The CLI limits the read before it allocates a full file. The library checks the supplied byte count before parsing.

Accepted syntax is global assignments, tables, string or number keys, quoted UTF-8 strings, numbers, nil, booleans, and comments. Function calls, expressions, metatables, concatenation, and executable statements are rejected. Duplicate keys and duplicate globals are rejected. Long bracket strings and non-UTF-8 strings are not supported. A caller must set an upload byte limit before it loads the request body.

## Data limits

The fixture is synthetic. No file from a running Forever client has been verified. All P0 client gates remain open.

Matching record counts do not prove that a scan is complete. Such a scan has `unknown` quality. A record-count mismatch has `partial` quality. The parser never sets `complete`. A separate, verified format policy must establish completeness before alerts or absence claims are allowed.

History does not store a realm in the current upstream schema. Its normalized realm is empty unless the input contains an explicit realm. The server must bind it to a configured and verified market. It must not infer a realm from another scan in the same file. History item IDs describe base items. They do not supply variant-level prices. Item keys from full records remain intact, including compact suffix and bonus fields.

Zero buyout means no buyout. Bid-only listings have no unit buyout. Full record prices are exact rational copper per unit. Inherited history prices are already rounded by AHDB and cannot recover the exact listing prices. Daily values remain separate from raw scan observations.

The scan fingerprint uses a versioned canonical encoding of realm, house, source time, format, declared counts, and sorted listings. Sorting keeps duplicate listings. Character names, seller names, device IDs and upload IDs are excluded. Identical independent scans in the same second cannot be distinguished when the source has no stable scan ID. Callers must retain upload provenance separately.
