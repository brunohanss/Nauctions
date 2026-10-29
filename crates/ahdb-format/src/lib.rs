//! Bounded SavedVariables data reader. No Lua interpreter or executable grammar.
use market_domain::*;
use serde::Serialize;
use std::collections::BTreeMap;
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub String);
type Result<T> = std::result::Result<T, Error>;
fn bad<T>(s: &str) -> Result<T> {
    Err(Error(s.into()))
}
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_string_bytes: usize,
    pub max_listings: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_bytes: 32 * 1024 * 1024,
            max_depth: 48,
            max_nodes: 1_000_000,
            max_string_bytes: 24 * 1024 * 1024,
            max_listings: 500_000,
        }
    }
}
#[derive(Debug, Clone)]
enum V {
    Nil,
    Bool(bool),
    Num(String),
    Str(String),
    Table(BTreeMap<String, V>, usize),
}
impl V {
    fn table(&self) -> Result<&BTreeMap<String, V>> {
        if let Self::Table(v, _) = self {
            Ok(v)
        } else {
            bad("expected table")
        }
    }
    fn string(&self) -> Result<&str> {
        if let Self::Str(v) = self {
            Ok(v)
        } else {
            bad("expected string")
        }
    }
    fn uint(&self) -> Result<u64> {
        if let Self::Num(v) = self {
            number(v)
        } else {
            bad("expected unsigned integer")
        }
    }
}
fn number(s: &str) -> Result<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return bad("invalid nonnegative integer");
    }
    let n: u64 = s.parse().map_err(|_| Error("integer overflow".into()))?;
    if n > i64::MAX as u64 {
        return bad("integer exceeds PostgreSQL BIGINT bound");
    }
    Ok(n)
}
struct Reader<'a> {
    b: &'a [u8],
    p: usize,
    n: usize,
    l: Limits,
    spans: BTreeMap<(usize, String), (usize, usize)>,
}
impl Reader<'_> {
    fn ws(&mut self) -> Result<()> {
        loop {
            while self.b.get(self.p).is_some_and(u8::is_ascii_whitespace) {
                self.p += 1
            }
            if self.b.get(self.p..self.p + 2) == Some(b"--") {
                self.p += 2;
                if self.b.get(self.p..self.p + 2) == Some(b"[[") {
                    self.p += 2;
                    while self.b.get(self.p..self.p + 2) != Some(b"]]") {
                        if self.p >= self.b.len() {
                            return bad("unterminated comment");
                        }
                        self.p += 1
                    }
                    self.p += 2
                } else {
                    while self.b.get(self.p).is_some_and(|c| *c != b'\n') {
                        self.p += 1
                    }
                }
            } else {
                return Ok(());
            }
        }
    }
    fn take(&mut self, c: u8) -> Result<()> {
        self.ws()?;
        if self.b.get(self.p) == Some(&c) {
            self.p += 1;
            Ok(())
        } else {
            bad("unexpected token")
        }
    }
    fn ident(&mut self) -> Result<String> {
        self.ws()?;
        let start = self.p;
        if !self
            .b
            .get(self.p)
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
        {
            return bad("expected identifier");
        }
        self.p += 1;
        while self
            .b
            .get(self.p)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            self.p += 1
        }
        if self.p - start > self.l.max_string_bytes {
            return bad("identifier limit");
        }
        Ok(String::from_utf8(self.b[start..self.p].to_vec()).unwrap())
    }
    fn string(&mut self) -> Result<String> {
        let q = self.b[self.p];
        self.p += 1;
        let mut out = Vec::new();
        loop {
            let c = *self
                .b
                .get(self.p)
                .ok_or_else(|| Error("unterminated string".into()))?;
            self.p += 1;
            if c == q {
                break;
            }
            if c == b'\\' {
                let e = *self
                    .b
                    .get(self.p)
                    .ok_or_else(|| Error("unterminated escape".into()))?;
                self.p += 1;
                match e {
                    b'a' => out.push(7),
                    b'b' => out.push(8),
                    b'f' => out.push(12),
                    b'v' => out.push(11),
                    b'\n' => out.push(b'\n'),
                    b'\r' => {
                        if self.b.get(self.p) == Some(&b'\n') {
                            self.p += 1;
                        }
                        out.push(b'\n');
                    }
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'\\' | b'\'' | b'"' => out.push(e),
                    b'0'..=b'9' => {
                        let mut n = (e - b'0') as u16;
                        for _ in 0..2 {
                            if let Some(d @ b'0'..=b'9') = self.b.get(self.p) {
                                n = n * 10 + (d - b'0') as u16;
                                self.p += 1
                            } else {
                                break;
                            }
                        }
                        if n > 255 {
                            return bad("escape overflow");
                        }
                        out.push(n as u8)
                    }
                    _ => return bad("unsupported string escape"),
                }
            } else {
                if c == b'\n' || c == b'\r' {
                    return bad("unescaped newline");
                }
                out.push(c)
            }
            if out.len() > self.l.max_string_bytes {
                return bad("string limit");
            }
        }
        String::from_utf8(out).map_err(|_| Error("invalid UTF-8 string".into()))
    }
    fn value(&mut self, d: usize) -> Result<V> {
        self.ws()?;
        self.n += 1;
        if d > self.l.max_depth.min(96) || self.n > self.l.max_nodes {
            return bad("structure limit");
        }
        match self.b.get(self.p).copied() {
            Some(b'"' | b'\'') => Ok(V::Str(self.string()?)),
            Some(b'{') => {
                let table_start = self.p;
                self.p += 1;
                let mut t = BTreeMap::new();
                let mut index = 1;
                loop {
                    self.ws()?;
                    if self.b.get(self.p) == Some(&b'}') {
                        self.p += 1;
                        return Ok(V::Table(t, table_start));
                    }
                    let key;
                    if self.b.get(self.p) == Some(&b'[') {
                        self.p += 1;
                        let k = self.value(d + 1)?;
                        key = match k {
                            V::Str(s) => {
                                if s.starts_with('\0') {
                                    return bad("unsupported NUL-prefixed key");
                                }
                                s
                            }
                            V::Num(s) => format!("\0{}", s),
                            _ => return bad("invalid table key"),
                        };
                        self.take(b']')?;
                        self.take(b'=')?;
                    } else {
                        let save = self.p;
                        if self
                            .b
                            .get(self.p)
                            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_')
                        {
                            let s = self.ident()?;
                            self.ws()?;
                            if self.b.get(self.p) == Some(&b'=') {
                                self.p += 1;
                                key = s;
                            } else {
                                self.p = save;
                                key = format!("\0{}", index);
                                index += 1;
                            }
                        } else {
                            key = format!("\0{}", index);
                            index += 1;
                        }
                    }
                    self.ws()?;
                    let start = self.p;
                    let v = self.value(d + 1)?;
                    self.spans
                        .insert((table_start, key.clone()), (start, self.p));
                    if t.insert(key, v).is_some() {
                        return bad("duplicate table key");
                    }
                    self.ws()?;
                    match self.b.get(self.p) {
                        Some(b',' | b';') => self.p += 1,
                        Some(b'}') => {}
                        _ => return bad("expected table separator"),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => {
                let start = self.p;
                self.p += 1;
                while self.b.get(self.p).is_some_and(|b| {
                    b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'E' | b'+' | b'-')
                }) {
                    self.p += 1
                }
                let s = std::str::from_utf8(&self.b[start..self.p]).unwrap();
                if s.parse::<f64>().ok().is_none_or(|x| !x.is_finite()) {
                    return bad("invalid number");
                }
                Ok(V::Num(s.into()))
            }
            Some(_) => match self.ident()?.as_str() {
                "nil" => Ok(V::Nil),
                "true" => Ok(V::Bool(true)),
                "false" => Ok(V::Bool(false)),
                _ => bad("executable Lua is forbidden"),
            },
            None => bad("unexpected end"),
        }
    }
}
fn get<'a>(t: &'a BTreeMap<String, V>, k: &str) -> Result<&'a V> {
    t.get(k).ok_or_else(|| Error(format!("missing {k}")))
}
#[derive(Debug, Default, Serialize)]
pub struct ParsedFile {
    pub scans: Vec<Scan>,
    /// Houses with imported history. Ingestion must exclude their recent and daily history.
    pub imported_history_markets: Vec<Market>,
    pub history: Vec<HistoryPoint>,
    pub daily: Vec<DailyPoint>,
    pub items: Vec<ItemMetadata>,
}
pub fn parse(b: &[u8]) -> Result<ParsedFile> {
    parse_with_limits(b, Limits::default())
}
pub fn parse_with_limits(b: &[u8], limits: Limits) -> Result<ParsedFile> {
    if b.len() > limits.max_bytes {
        return bad("file size limit");
    }
    let mut r = Reader {
        b,
        p: 0,
        n: 0,
        l: limits,
        spans: BTreeMap::new(),
    };
    let mut roots = BTreeMap::new();
    r.ws()?;
    while r.p < b.len() {
        let name = r.ident()?;
        r.take(b'=')?;
        let v = r.value(0)?;
        if roots.insert(name, v).is_some() {
            return bad("duplicate global");
        }
        r.ws()?;
        if r.b.get(r.p) == Some(&b';') {
            r.p += 1;
            r.ws()?;
        }
    }
    let root = roots
        .get("AuctionDBForeverSaved")
        .or_else(|| roots.get("AuctionDBSaved"))
        .or_else(|| roots.get("AuctionDB"))
        .or_else(|| roots.get("AHDB"));
    let sv = if let Some(v) = root {
        v.table()?
    } else {
        return bad("missing AHDB SavedVariables root");
    };
    decode(sv, limits)
}
/// Expand run-length encoded colons without discarding suffix, gem, enchant or bonus fields.
/// The compact key is retained on normalized records as its source identity.
pub fn expand_item_key(key: &str) -> Result<String> {
    if !key.starts_with('i') || key.len() > 1024 {
        return bad("invalid compact item key");
    }
    let mut result = String::from("item:");
    for c in key[1..].chars() {
        match c {
            '0'..='9' | '-' | ':' => result.push(c),
            ';'..='Z' => {
                for _ in 0..(c as u32 - 57) {
                    result.push(':');
                }
            }
            _ => return bad("invalid compact item key character"),
        }
    }
    item_id(key)?;
    Ok(result)
}
fn item_id(key: &str) -> Result<u64> {
    let s = key.strip_prefix('i').unwrap_or(key);
    let id = s.split(|c: char| !c.is_ascii_digit()).next().unwrap_or("");
    let n = number(id)?;
    if n == 0 {
        return bad("zero item id");
    }
    Ok(n)
}
fn decode(sv: &BTreeMap<String, V>, l: Limits) -> Result<ParsedFile> {
    let mut out = ParsedFile::default();
    if let Some(marker) = sv.get("nauctionsSync") {
        let m = marker.table()?;
        if m.len() != 3 || get(m, "schema_version")?.uint()? != 1 {
            return bad("unsupported nauctionsSync marker schema");
        }
        let market = Market {
            realm: get(m, "realm")?.string()?.into(),
            house: get(m, "house")?.string()?.into(),
        };
        valid_market(&market)?;
        if let Some(realm) = sv.get("realm") {
            if realm.string()? != market.realm {
                return bad("sync marker realm mismatch");
            }
        }
        if let Some(houses) = sv.get("houses") {
            if let Some(h) = houses.table()?.get(&market.house) {
                if let Some(realm) = h.table()?.get("realm") {
                    if realm.string()? != market.realm {
                        return bad("sync marker house realm mismatch");
                    }
                }
            } else {
                return bad("sync marker house missing");
            }
        } else {
            return bad("sync marker houses missing");
        }
        out.imported_history_markets.push(market);
    }
    if !sv.contains_key("ah") && !sv.contains_key("houses") && !sv.contains_key("itemDB_2") {
        return bad("no supported AHDB data fields");
    }
    let mut records = 0usize;
    if let Some(ah) = sv.get("ah") {
        for v in ah.table()?.values() {
            let t = v.table()?;
            if get(t, "dataFormatVersion")?.uint()? != 6 {
                return bad("unsupported full scan format (expected 6)");
            }
            let market = Market {
                realm: get(t, "realm")?.string()?.into(),
                house: get(t, "faction")?.string()?.into(),
            };
            valid_market(&market)?;
            let mut listings = Vec::new();
            let mut keys = std::collections::BTreeSet::new();
            for group in get(t, "data")?.string()?.split_whitespace() {
                let mut segments = group.split('!');
                let key = segments.next().unwrap();
                expand_item_key(key)?;
                if !group.contains('!') {
                    return bad("item group has no listings");
                }
                keys.insert(key);
                for seller in segments {
                    let (_, auctions) = seller
                        .split_once('/')
                        .ok_or_else(|| Error("invalid seller group".into()))?;
                    for a in auctions.split('&') {
                        records += 1;
                        if records > l.max_listings {
                            return bad("decoded record limit");
                        }
                        let fields: Vec<_> = a.split(',').collect();
                        if fields.len() != 5 {
                            return bad("listing must have five fields");
                        }
                        let nums: Vec<u64> = fields
                            .iter()
                            .map(|s| if s.is_empty() { Ok(0) } else { number(s) })
                            .collect::<Result<_>>()?;
                        if nums[1] == 0 || !(1..=4).contains(&nums[0]) {
                            return bad("invalid stack or time-left band");
                        }
                        listings.push(Listing {
                            item_key: key.into(),
                            stack_count: nums[1],
                            min_bid_copper: nums[2],
                            buyout_total_copper: (nums[3] > 0).then_some(nums[3]),
                            current_bid_copper: nums[4],
                            time_left_band: nums[0] as u8,
                        });
                        if listings.len() > l.max_listings {
                            return bad("listing limit");
                        }
                    }
                }
            }
            let declared_count = get(t, "count")?.uint()?;
            let declared_items_count = get(t, "itemsCount")?.uint()?;
            let quality = if declared_count != listings.len() as u64
                || declared_items_count != keys.len() as u64
            {
                ScanQuality::Partial
            } else {
                ScanQuality::Unknown
            };
            let mut scan = Scan {
                market,
                observed_at: timestamp(get(t, "ts")?.uint()?)?,
                format_version: 6,
                declared_count,
                declared_items_count,
                quality,
                listings,
                fingerprint: String::new(),
            };
            scan.fingerprint = scan.canonical_fingerprint();
            out.scans.push(scan)
        }
    }
    if let Some(idb) = sv.get("itemDB_2") {
        let t = idb.table()?;
        if get(t, "_formatVersion_")?.uint()? != 5 {
            return bad("unsupported item metadata format");
        }
        let locale = t
            .get("_locale_")
            .map(V::string)
            .transpose()?
            .map(str::to_owned);
        for (key, v) in t {
            if key.starts_with('_') {
                continue;
            }
            let raw = v.string()?;
            let p = raw
                .find('|')
                .ok_or_else(|| Error("missing item link".into()))?;
            let meta = &raw[..p];
            let mut m = ItemMetadata {
                item_key: key.clone(),
                base_item_id: item_id(key)?,
                link: raw[p..].into(),
                locale: locale.clone(),
                sell_price: None,
                stack_count: None,
                class_id: None,
                subclass_id: None,
                rarity: None,
                min_level: None,
            };
            expand_item_key(key)?;
            if !meta.is_empty() {
                let n: Vec<_> = meta.split(',').map(number).collect::<Result<_>>()?;
                if n.len() != 6 {
                    return bad("invalid item metadata");
                }
                m.sell_price = Some(n[0]);
                m.stack_count = Some(n[1]);
                m.class_id = Some(n[2]);
                m.subclass_id = Some(n[3]);
                m.rarity = Some(n[4]);
                m.min_level = Some(n[5]);
            }
            out.items.push(m)
        }
    }
    if let Some(houses) = sv.get("houses") {
        for (house, h) in houses.table()? {
            let t = h.table()?;
            let realm = t
                .get("realm")
                .or_else(|| sv.get("realm"))
                .map(V::string)
                .transpose()?
                .unwrap_or("");
            let market = Market {
                realm: realm.into(),
                house: house.clone(),
            };
            for (field, daily) in [("items", false), ("daily", true)] {
                if let Some(data) = t.get(field) {
                    for (key, v) in data.table()? {
                        let key = key.strip_prefix('\0').unwrap_or(key);
                        if number(key)? == 0 {
                            return bad("zero history item id");
                        }
                        for point in v.string()?.split_whitespace() {
                            records += 1;
                            if records > l.max_listings {
                                return bad("decoded record limit");
                            }
                            let n: Vec<_> = point.split(',').map(number).collect::<Result<_>>()?;
                            if n.len() != if daily { 5 } else { 4 } {
                                return bad("invalid history point");
                            }
                            let ts = timestamp(n[0])?;
                            if daily {
                                if n[4] == 0 {
                                    return bad("daily scan count is zero");
                                }
                                out.daily.push(DailyPoint {
                                    market: market.clone(),
                                    item_key: key.to_string(),
                                    observed_at: ts,
                                    utc_day: ts / 86400,
                                    min_unit_copper: (n[1] > 0).then_some(n[1]),
                                    median_unit_copper: (n[2] > 0).then_some(n[2]),
                                    average_quantity: n[3],
                                    scan_count: n[4],
                                })
                            } else {
                                out.history.push(HistoryPoint {
                                    market: market.clone(),
                                    item_key: key.to_string(),
                                    observed_at: ts,
                                    min_unit_copper: (n[1] > 0).then_some(n[1]),
                                    median_unit_copper: (n[2] > 0).then_some(n[2]),
                                    quantity: n[3],
                                })
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(out)
}
fn timestamp(n: u64) -> Result<i64> {
    i64::try_from(n).map_err(|_| Error("timestamp overflow".into()))
}
fn valid_market(m: &Market) -> Result<()> {
    if m.realm.trim().is_empty() || !matches!(m.house.as_str(), "Alliance" | "Horde" | "Neutral") {
        bad("invalid realm or auction house")
    } else {
        Ok(())
    }
}

/// Replace only the selected history spans and insert the durable sync marker.
/// All other source bytes remain unchanged. Requires AHDB full-scan recording.
pub fn merge_history(
    input: &[u8],
    bundle: &HistoryExport,
    expected_realm: &str,
    expected_house: &str,
) -> Result<Vec<u8>> {
    let limits = Limits::default();
    if input.len() > limits.max_bytes
        || bundle.schema_version != 1
        || bundle.generated_at <= 0
        || bundle.generated_at > 9_007_199_254_740_991
        || bundle.market.realm != expected_realm
        || bundle.market.house != expected_house
    {
        return bad("invalid export schema, time, size or market binding");
    }
    valid_market(&bundle.market)?;
    if bundle.points.len().saturating_add(bundle.daily.len()) > limits.max_listings {
        return bad("export record limit");
    }
    let parsed = parse(input)?;
    if parsed
        .scans
        .iter()
        .any(|s| s.market.realm != expected_realm)
    {
        return bad("full scan belongs to another realm");
    }
    let mut r = Reader {
        b: input,
        p: 0,
        n: 0,
        l: limits,
        spans: BTreeMap::new(),
    };
    let mut roots = BTreeMap::new();
    r.ws()?;
    while r.p < input.len() {
        let name = r.ident()?;
        r.take(b'=')?;
        let value = r.value(0)?;
        if roots.insert(name, value).is_some() {
            return bad("duplicate global");
        }
        r.ws()?;
        if input.get(r.p) == Some(&b';') {
            r.p += 1;
            r.ws()?;
        }
    }
    let root_value = [
        "AuctionDBForeverSaved",
        "AuctionDBSaved",
        "AuctionDB",
        "AHDB",
    ]
    .iter()
    .find_map(|n| roots.get(*n))
    .ok_or_else(|| Error("missing root".into()))?;
    let root = root_value.table()?;
    let root_start = match root_value {
        V::Table(_, start) => *start,
        _ => unreachable!(),
    };
    if !matches!(
        root.get("settings")
            .and_then(|s| s.table().ok())
            .and_then(|s| s.get("recordScans")),
        Some(V::Bool(true))
    ) {
        return bad("Enable Record full scans in AHDB settings before history sync");
    }
    if parsed
        .imported_history_markets
        .iter()
        .any(|m| m != &bundle.market)
    {
        return bad("sync marker market conflicts with selected market");
    }
    if let Some(realm) = root.get("realm") {
        if realm.string()? != expected_realm {
            return bad("root realm mismatch");
        }
    }
    let houses = get(root, "houses")?;
    let houses_start = match houses {
        V::Table(_, start) => *start,
        _ => return bad("expected houses table"),
    };
    let empty_house = BTreeMap::new();
    let house = houses.table()?.get(expected_house);
    let (h, start) = match house {
        Some(V::Table(t, start)) => (t, Some(*start)),
        None => (&empty_house, None),
        _ => return bad("expected house table"),
    };
    if let Some(realm) = h.get("realm") {
        if realm.string()? != expected_realm {
            return bad("house realm mismatch");
        }
    }
    // Existing history is realm-less in AHDB. The caller's explicit binding is mandatory.
    let mut recent: BTreeMap<u64, BTreeMap<i64, Vec<u64>>> = BTreeMap::new();
    let mut daily: BTreeMap<u64, BTreeMap<i64, Vec<u64>>> = BTreeMap::new();
    let validate = |id: u64, ts: i64, values: &[u64]| -> Result<()> {
        if id == 0
            || ts < 0
            || ts > bundle.generated_at
            || values.iter().any(|n| *n > 9_007_199_254_740_991)
            || id > 9_007_199_254_740_991
        {
            return bad("export value outside exact Lua bounds");
        }
        Ok(())
    };
    for p in &bundle.points {
        let v = vec![
            p.observed_at as u64,
            p.min_unit_copper,
            p.median_unit_copper,
            p.quantity,
        ];
        validate(p.item_id, p.observed_at, &v)?;
        recent
            .entry(p.item_id)
            .or_default()
            .entry(p.observed_at)
            .or_insert(v);
    }
    for p in &bundle.daily {
        let v = vec![
            p.observed_at as u64,
            p.min_unit_copper,
            p.median_unit_copper,
            p.quantity,
            p.scan_count,
        ];
        validate(p.item_id, p.observed_at, &v)?;
        if p.scan_count == 0 {
            return bad("zero daily scan count");
        }
        daily
            .entry(p.item_id)
            .or_default()
            .entry(p.observed_at / 86400)
            .or_insert(v);
    }
    let mut local_keys: BTreeMap<(bool, u64), String> = BTreeMap::new();
    for (field, target, is_daily) in [("items", &mut recent, false), ("daily", &mut daily, true)] {
        if let Some(data) = h.get(field) {
            for (key, v) in data.table()? {
                let id = number(key.strip_prefix('\0').unwrap_or(key))?;
                let spelling = if key.starts_with('\0') {
                    format!("[{}]", id)
                } else {
                    format!("[\"{}\"]", key)
                };
                if local_keys.insert((is_daily, id), spelling).is_some() {
                    return bad("ambiguous numeric and string history item keys");
                }
                for point in v.string()?.split_whitespace() {
                    let nums = point.split(',').map(number).collect::<Result<Vec<_>>>()?;
                    if nums.len() != if is_daily { 5 } else { 4 }
                        || nums.iter().any(|n| *n > 9_007_199_254_740_991)
                    {
                        return bad("invalid local history");
                    }
                    let ts = nums[0] as i64;
                    target
                        .entry(id)
                        .or_default()
                        .insert(if is_daily { ts / 86400 } else { ts }, nums);
                }
            }
        }
    }
    let mut first_day = bundle.generated_at / 86400 - 6;
    // AHDB uses whole days of the house's last 30 scans.
    let mut scans = Vec::new();
    if let Some(v) = h.get("scans") {
        for scan in v.table()?.values() {
            let t = scan.table()?;
            let ts = get(t, "\u{0}1")?.uint()?;
            scans.push(ts as i64);
        }
    }
    scans.sort_unstable();
    scans.dedup();
    if scans.len() >= 30 {
        first_day = first_day.max(scans[scans.len() - 30] / 86400);
    }
    for (id, points) in &mut recent {
        let mut old: BTreeMap<i64, Vec<Vec<u64>>> = BTreeMap::new();
        points.retain(|ts, p| {
            if ts / 86400 < first_day {
                old.entry(ts / 86400).or_default().push(p.clone());
                false
            } else {
                true
            }
        });
        for (day, rows) in old {
            let n = rows.len() as u64;
            let mut meds: Vec<_> = rows.iter().map(|p| p[2]).filter(|v| *v > 0).collect();
            meds.sort_unstable();
            let min = rows
                .iter()
                .map(|p| p[1])
                .filter(|v| *v > 0)
                .min()
                .unwrap_or(0);
            let qty = (rows.iter().map(|p| p[3] as u128).sum::<u128>() + n as u128 / 2) / n as u128;
            let roll = vec![
                rows.last().unwrap()[0],
                min,
                if meds.is_empty() {
                    0
                } else {
                    meds[(meds.len() - 1) / 2]
                },
                qty as u64,
                n,
            ];
            // A stored daily aggregate has priority: its underlying observations are unknown.
            daily.entry(*id).or_default().entry(day).or_insert(roll);
        }
    }
    for rows in daily.values_mut() {
        rows.retain(|_, v| v[0] as i64 >= bundle.generated_at.saturating_sub(180 * 86400));
    }
    fn encode(
        data: &BTreeMap<u64, BTreeMap<i64, Vec<u64>>>,
        keys: &BTreeMap<(bool, u64), String>,
        is_daily: bool,
    ) -> String {
        let mut out = String::from("{");
        for (id, rows) in data {
            if rows.is_empty() {
                continue;
            }
            out.push_str(&format!(
                "{}=\"",
                keys.get(&(is_daily, *id))
                    .cloned()
                    .unwrap_or_else(|| format!("[{}]", id))
            ));
            out.push_str(
                &rows
                    .values()
                    .map(|v| v.iter().map(u64::to_string).collect::<Vec<_>>().join(","))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            out.push_str("\",");
        }
        out.push('}');
        out
    }
    let mut edits = Vec::new();
    let mut insert = String::new();
    let items = encode(&recent, &local_keys, false);
    let daily = encode(&daily, &local_keys, true);
    if let Some(start) = start {
        for (field, value) in [("items", items), ("daily", daily)] {
            if let Some((a, b)) = r.spans.get(&(start, field.into())) {
                edits.push((*a, *b, value));
            } else {
                insert.push_str(&format!("{}={},", field, value));
            }
        }
        if !insert.is_empty() {
            edits.push((start + 1, start + 1, insert));
        }
    } else {
        edits.push((
            houses_start + 1,
            houses_start + 1,
            format!(
                "{}={{items={},daily={},scans={{}}}},",
                expected_house, items, daily
            ),
        ));
    }
    // This marker survives addon compaction, so downloaded observations cannot feed back as native history.
    if !root.contains_key("nauctionsSync") {
        fn quote(value: &str) -> String {
            let mut out = String::from("\"");
            for byte in value.bytes() {
                out.push_str(&format!("\\{:03}", byte));
            }
            out.push('"');
            out
        }
        edits.push((
            root_start + 1,
            root_start + 1,
            format!(
                "nauctionsSync={{schema_version=1,realm={},house={}}},",
                quote(expected_realm),
                quote(expected_house)
            ),
        ));
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut output = input.to_vec();
    for (a, b, value) in edits {
        output.splice(a..b, value.bytes());
    }
    parse(&output)?;
    Ok(output)
}
