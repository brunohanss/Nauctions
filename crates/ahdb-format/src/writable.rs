//! Strict target checks for the pinned Forever history writer.
//! Read compatibility intentionally accepts more shapes than write compatibility.
use super::*;

pub(super) fn house<'a>(
    roots: &'a BTreeMap<String, V>,
    name: &str,
) -> Result<(&'a V, &'a BTreeMap<String, V>, usize)> {
    let candidates = [
        "AuctionDBForeverSaved",
        "AuctionDBSaved",
        "AuctionDB",
        "AHDB",
    ];
    if candidates
        .iter()
        .filter(|key| roots.contains_key(**key))
        .count()
        != 1
    {
        return bad("history sync requires exactly one supported SavedVariables root");
    }
    let root_value = get(roots, "AuctionDBForeverSaved")?;
    let root = root_value.table()?;
    let value = get(get(root, "houses")?.table()?, name)?;
    let V::Table(house, start) = value else {
        return bad("expected existing house table; history sync does not repair game data");
    };
    for field in ["items", "daily"] {
        for (key, value) in get(house, field)?.table()? {
            let Some(key) = key.strip_prefix('\0') else {
                return bad("history sync requires numeric item keys for addon lookup");
            };
            let id = number(key)?;
            if id == 0 || id > 9_007_199_254_740_991 {
                return bad("history item ID outside exact Lua bounds");
            }
            value.string()?;
        }
    }
    let scans = get(house, "scans")?.table()?;
    let mut previous = 0;
    // Index lookup, rather than BTreeMap iteration, models Lua's dense sequence.
    for index in 1..=scans.len() {
        let scan = get(scans, &format!("\0{index}"))?.table()?;
        if scan.len() != 3 {
            return bad("history scan row must contain timestamp, auction count and item count");
        }
        let ts = get(scan, "\u{0}1")?.uint()?;
        let auctions = get(scan, "\u{0}2")?.uint()?;
        let items = get(scan, "\u{0}3")?.uint()?;
        if ts <= previous
            || ts > 9_007_199_254_740_991
            || items > auctions
            || auctions > 9_007_199_254_740_991
        {
            return bad("invalid or unordered history scan row");
        }
        previous = ts;
    }
    Ok((root_value, house, *start))
}
