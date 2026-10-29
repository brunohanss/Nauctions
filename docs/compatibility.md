# Source compatibility and release gates

Target client: WoW Forever **1.60.1.70630**. The user confirmed that the game is not released and no real scan files are available.

The implementation uses the published AHDB format and synthetic test files. These tests do not prove game compatibility. Do not mark E00 or the real-client pilot complete until real evidence exists.

Full scan format: version 6. Item metadata: version 5. Source references:

- https://github.com/mooreatv/AuctionDB/blob/master/AuctionDB/Record.lua
- https://github.com/mooreatv/AuctionDB/blob/master/AuctionDB/History.lua

Recorded row counts can detect some partial scans. Matching counts do not prove complete market coverage. The default parser therefore marks these scans as unknown. Enable trusted complete scans only after a live full-scan and interrupted-scan comparison proves this policy for the pinned addon version.

Before release, record the AHDB commit, realm, house, file size, save duration, and fixture SHA-256. Verify reload, logout, restart, two account paths, interrupted scans, bid-only listings, item variants, and manual prices. Obtain contributor consent before retaining any raw sample. Remove player identifiers before committing fixtures.

Real-client validation, two-player multi-day testing, production hosting, and owner launch sign-off remain external release gates. Continue software implementation and synthetic validation while these gates are pending.

The optional reverse synchronization feature is now implemented. It requires the user to enable AHDB full-scan recording and select the realm/house. It preserves source settings, trust entries, original full scans, and other houses. It adds a `nauctionsSync` marker and updates only the selected house's recent/daily history. The server ignores marked history when the file returns; original full scans remain eligible contributions. Verify that the released addon preserves this marker across save/reload before enabling sync for a pilot. Test the exact Windows game executable with the process guard. No real game file has been changed during development.

## Pinned upstream schema reference

On 27 October 2026, the GitHub repository API resolved `master` to commit `9a92cb047819fbf35219df58ef5dd3e6a28602c4` (25 October 2026, 20:13:49 UTC). The parser reference is this commit, not a moving branch:

- [Record.lua](https://github.com/mooreatv/AuctionDB/blob/9a92cb047819fbf35219df58ef5dd3e6a28602c4/AuctionDB/Record.lua)
- [History.lua](https://github.com/mooreatv/AuctionDB/blob/9a92cb047819fbf35219df58ef5dd3e6a28602c4/AuctionDB/History.lua)
- [AuctionDB.lua](https://github.com/mooreatv/AuctionDB/blob/9a92cb047819fbf35219df58ef5dd3e6a28602c4/AuctionDB/AuctionDB.lua)

The files were read at the pinned commit through the GitHub connector. `AuctionDB.lua` identifies `AuctionDBForeverSaved` as the SavedVariables global. Its history houses contain no realm field. A configured and verified server market must supply that missing context. History uses base item IDs and does not prove variant-level prices.

This pin proves the schema source used for implementation. It does not prove that the target client loads the addon or writes this format. Record the installed release separately when the real-client gate can run.
