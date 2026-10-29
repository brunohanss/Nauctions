use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
pub mod crafting;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Market {
    pub realm: String,
    pub house: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Listing {
    pub item_key: String,
    pub stack_count: u64,
    pub buyout_total_copper: Option<u64>,
    pub min_bid_copper: u64,
    pub current_bid_copper: u64,
    pub time_left_band: u8,
}
impl Listing {
    pub fn unit_buyout(&self) -> Option<UnitPrice> {
        UnitPrice::new(
            self.buyout_total_copper.filter(|n| *n > 0)?,
            self.stack_count,
        )
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanQuality {
    Unknown,
    Partial,
    Complete,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scan {
    pub market: Market,
    pub observed_at: i64,
    pub format_version: u32,
    pub declared_count: u64,
    pub declared_items_count: u64,
    pub quality: ScanQuality,
    pub listings: Vec<Listing>,
    pub fingerprint: String,
}
impl Scan {
    /// Content identity excludes character names and transport provenance. Identical independent scans in the same second remain ambiguous.
    pub fn canonical_fingerprint(&self) -> String {
        let mut listings = self.listings.clone();
        listings.sort();
        let bytes = serde_json::to_vec(&(
            "ahdb-scan-v1",
            &self.market,
            self.observed_at,
            self.format_version,
            self.declared_count,
            self.declared_items_count,
            listings,
        ))
        .expect("serializable scan");
        format!("{:x}", Sha256::digest(bytes))
    }
}
/// Exact copper per unit. API integers are decimal strings to preserve browser precision.
/// `rounded_copper` is a whole-copper convenience; the dashboard rounds to 0.01 copper at display time.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct UnitPrice {
    #[serde(rename = "numerator", serialize_with = "decimal_string")]
    total_copper: u64,
    #[serde(rename = "denominator", serialize_with = "decimal_string")]
    count: u64,
}
impl<'de> Deserialize<'de> for UnitPrice {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Fields {
            #[serde(rename = "numerator", deserialize_with = "decimal_u64")]
            total_copper: u64,
            #[serde(rename = "denominator", deserialize_with = "decimal_u64")]
            count: u64,
        }
        let value = Fields::deserialize(deserializer)?;
        Self::new(value.total_copper, value.count)
            .ok_or_else(|| serde::de::Error::custom("unit price count must be positive"))
    }
}
fn decimal_string<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
fn decimal_u64<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Decimal {
        Number(u64),
        String(String),
    }
    match Decimal::deserialize(deserializer)? {
        Decimal::Number(n) => Ok(n),
        Decimal::String(s) => {
            if s.is_empty() || !s.bytes().all(|c| c.is_ascii_digit()) {
                return Err(serde::de::Error::custom("invalid unsigned decimal"));
            }
            s.parse().map_err(serde::de::Error::custom)
        }
    }
}
impl UnitPrice {
    pub fn new(total_copper: u64, count: u64) -> Option<Self> {
        (count > 0).then_some(Self {
            total_copper,
            count,
        })
    }
    pub fn numerator(self) -> u64 {
        self.total_copper
    }
    pub fn denominator(self) -> u64 {
        self.count
    }
    pub fn rounded_copper(self) -> u64 {
        ((self.total_copper as u128 + self.count as u128 / 2) / self.count as u128) as u64
    }
}
impl PartialEq for UnitPrice {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for UnitPrice {}
impl PartialOrd for UnitPrice {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for UnitPrice {
    fn cmp(&self, o: &Self) -> Ordering {
        ((self.total_copper as u128) * (o.count as u128))
            .cmp(&((o.total_copper as u128) * (self.count as u128)))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryPoint {
    pub market: Market,
    pub item_key: String,
    pub observed_at: i64,
    pub min_unit_copper: Option<u64>,
    pub median_unit_copper: Option<u64>,
    pub quantity: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyPoint {
    pub market: Market,
    pub item_key: String,
    pub observed_at: i64,
    pub utc_day: i64,
    pub min_unit_copper: Option<u64>,
    pub median_unit_copper: Option<u64>,
    pub average_quantity: u64,
    pub scan_count: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMetadata {
    pub item_key: String,
    pub base_item_id: u64,
    pub link: String,
    pub locale: Option<String>,
    pub sell_price: Option<u64>,
    pub stack_count: Option<u64>,
    pub class_id: Option<u64>,
    pub subclass_id: Option<u64>,
    pub rarity: Option<u64>,
    pub min_level: Option<u64>,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_money() {
        assert_eq!(UnitPrice::new(20000, 20).unwrap().rounded_copper(), 1000);
        assert!(UnitPrice::new(10, 3).unwrap() > UnitPrice::new(3, 1).unwrap());
        assert_eq!(UnitPrice::new(u64::MAX, 2), UnitPrice::new(u64::MAX, 2));
        assert!(UnitPrice::new(1, 0).is_none());
    }
}

#[cfg(test)]
mod adversarial_tests {
    use super::*;
    #[test]
    fn rational_deserialization_enforces_denominator() {
        for json in [
            r#"{"numerator":1,"denominator":0}"#,
            r#"{"numerator":1,"denominator":-1}"#,
            r#"{"numerator":1,"denominator":0.5}"#,
            r#"{"numerator":1}"#,
        ] {
            assert!(serde_json::from_str::<UnitPrice>(json).is_err(), "{json}");
        }
        let p: UnitPrice = serde_json::from_str(
            r#"{"numerator":18446744073709551615,"denominator":18446744073709551615}"#,
        )
        .unwrap();
        assert_eq!(p, UnitPrice::new(1, 1).unwrap());
        assert_eq!(p.rounded_copper(), 1);
        assert_eq!(
            serde_json::to_string(&UnitPrice::new(7, 2).unwrap()).unwrap(),
            r#"{"numerator":"7","denominator":"2"}"#
        );
        let large = UnitPrice::new(9007199254740993, 1).unwrap();
        let encoded = serde_json::to_string(&large).unwrap();
        assert_eq!(
            encoded,
            r#"{"numerator":"9007199254740993","denominator":"1"}"#
        );
        assert_eq!(serde_json::from_str::<UnitPrice>(&encoded).unwrap(), large);
        for invalid in [
            r#"{"numerator":"1","denominator":"0"}"#,
            r#"{"numerator":"-1","denominator":"1"}"#,
            r#"{"numerator":"1e2","denominator":"1"}"#,
        ] {
            assert!(serde_json::from_str::<UnitPrice>(invalid).is_err())
        }
    }
    #[test]
    fn extrema_order_without_float_rounding() {
        let high = UnitPrice::new(u64::MAX, u64::MAX - 1).unwrap();
        let low = UnitPrice::new(u64::MAX - 1, u64::MAX).unwrap();
        assert!(high > UnitPrice::new(1, 1).unwrap());
        assert!(low < UnitPrice::new(1, 1).unwrap());
        assert!(high > low);
        assert_eq!(
            UnitPrice::new(u64::MAX, 1).unwrap().rounded_copper(),
            u64::MAX
        );
        assert_eq!(UnitPrice::new(1, 2).unwrap().rounded_copper(), 1);
    }
}

/// Portable history bundle. Decimal strings preserve all integer precision.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryExport {
    pub schema_version: u32,
    pub market: Market,
    pub generated_at: i64,
    pub points: Vec<ExportPoint>,
    pub daily: Vec<ExportDaily>,
}

/// One immutable response. A digest detects corruption; HTTPS authenticates the sender.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistorySnapshot {
    pub schema_version: u32,
    pub snapshot_id: String,
    pub expires_at: i64,
    pub point_count: usize,
    pub daily_count: usize,
    pub sha256: String,
    pub history: HistoryExport,
}
impl HistorySnapshot {
    pub fn new(history: HistoryExport) -> Self {
        let sha256 = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&history).expect("history serializes"))
        );
        Self {
            schema_version: 2,
            snapshot_id: sha256.clone(),
            expires_at: history.generated_at + 900,
            point_count: history.points.len(),
            daily_count: history.daily.len(),
            sha256,
            history,
        }
    }
    pub fn verify(&self, now: i64) -> bool {
        self.schema_version == 2
            && self.history.schema_version == 1
            && self.history.generated_at > 0
            && self.history.generated_at.abs_diff(now) <= 300
            && self.expires_at > now
            && self.expires_at > self.history.generated_at
            && self.expires_at - self.history.generated_at <= 900
            && self.point_count == self.history.points.len()
            && self.daily_count == self.history.daily.len()
            && self.point_count.saturating_add(self.daily_count) <= 100_000
            && self.sha256 == self.snapshot_id
            && self.sha256
                == format!(
                    "{:x}",
                    Sha256::digest(serde_json::to_vec(&self.history).expect("history serializes"))
                )
            && self.history.points.iter().all(|p| {
                p.observed_at > 0
                    && p.observed_at <= self.history.generated_at
                    && p.observed_at >= (self.history.generated_at.div_euclid(86400) - 179) * 86400
            })
            && self.history.daily.iter().all(|p| {
                p.scan_count > 0
                    && p.observed_at > 0
                    && p.observed_at <= self.history.generated_at
                    && p.observed_at >= (self.history.generated_at.div_euclid(86400) - 179) * 86400
            })
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn snapshots_reject_tampering_counts_clock_skew_and_expiry() {
        let s = HistorySnapshot::new(HistoryExport {
            schema_version: 1,
            market: Market {
                realm: "Test".into(),
                house: "Alliance".into(),
            },
            generated_at: 1_700_000_000,
            points: vec![],
            daily: vec![],
        });
        assert!(s.verify(1_700_000_001));
        let mut changed = s.clone();
        changed.history.market.realm = "Other".into();
        assert!(!changed.verify(1_700_000_001));
        let mut changed = s.clone();
        changed.point_count = 1;
        assert!(!changed.verify(1_700_000_001));
        let mut changed = s.clone();
        changed.expires_at = 1_700_000_000;
        assert!(!changed.verify(1_700_000_001));
        assert!(!s.verify(1_700_001_000));
        assert!(!s.verify(1_699_999_000));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportPoint {
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub item_id: u64,
    pub observed_at: i64,
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub min_unit_copper: u64,
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub median_unit_copper: u64,
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub quantity: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportDaily {
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub item_id: u64,
    pub observed_at: i64,
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub min_unit_copper: u64,
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub median_unit_copper: u64,
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub quantity: u64,
    #[serde(serialize_with = "decimal_string", deserialize_with = "decimal_u64")]
    pub scan_count: u64,
}
