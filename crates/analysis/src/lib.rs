//! Exact snapshot metrics and conservative advisory signals.
pub mod forecast;
pub mod strategy;
use chrono::{DateTime, Days, TimeZone};
use chrono_tz::Europe::Paris;
use market_domain::{Listing, UnitPrice};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct Error(pub &'static str);
type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotMetrics {
    pub min_buyout: Option<UnitPrice>,
    pub median_buyout: Option<UnitPrice>,
    pub total_units: u64,
    pub listing_count: u64,
}
/// Each call covers one snapshot only. Bid-only units do not contribute to buyout medians.
pub fn snapshot(listings: &[Listing]) -> Result<BTreeMap<String, SnapshotMetrics>> {
    let mut groups: BTreeMap<String, Vec<&Listing>> = BTreeMap::new();
    for l in listings {
        if l.stack_count == 0 {
            return Err(Error("zero listing stack"));
        }
        groups.entry(l.item_key.clone()).or_default().push(l);
    }
    let mut out = BTreeMap::new();
    for (key, ls) in groups {
        let total_units = ls.iter().try_fold(0u64, |sum, l| {
            sum.checked_add(l.stack_count)
                .ok_or(Error("quantity overflow"))
        })?;
        if total_units > i64::MAX as u64 {
            return Err(Error("quantity exceeds storage bound"));
        }
        let listing_count = ls.len() as u64;
        let mut buyouts: Vec<_> = ls
            .iter()
            .filter_map(|l| {
                l.buyout_total_copper
                    .filter(|n| *n > 0)
                    .and_then(|n| UnitPrice::new(n, l.stack_count))
                    .map(|p| (p, l.stack_count))
            })
            .collect();
        buyouts.sort_by_key(|(p, _)| *p);
        let weight = buyouts.iter().try_fold(0u64, |sum, (_, q)| {
            sum.checked_add(*q).ok_or(Error("quantity overflow"))
        })?;
        let mut acc = 0u64;
        let median_buyout = buyouts.iter().find_map(|(p, q)| {
            acc += q;
            (acc as u128 * 2 >= weight as u128).then_some(*p)
        });
        out.insert(
            key,
            SnapshotMetrics {
                min_buyout: buyouts.first().map(|(p, _)| *p),
                median_buyout,
                total_units,
                listing_count,
            },
        );
    }
    Ok(out)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistorySample {
    pub observed_at: i64,
    pub price: UnitPrice,
    pub source_id: String,
    pub eligible: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub price: Option<UnitPrice>,
    pub day_count: usize,
    pub source_count: usize,
}
fn median(mut v: Vec<UnitPrice>) -> Option<UnitPrice> {
    if v.is_empty() {
        None
    } else {
        v.sort();
        Some(v[(v.len() - 1) / 2])
    }
}
/// UTC instants for the Paris calendar day containing ts. Days can have 23, 24 or 25 hours.
pub fn paris_day_bounds(ts: i64) -> Result<(i64, i64)> {
    let utc = DateTime::from_timestamp(ts, 0).ok_or(Error("invalid timestamp"))?;
    let day = utc.with_timezone(&Paris).date_naive();
    let next = day
        .checked_add_days(Days::new(1))
        .ok_or(Error("date overflow"))?;
    let start = Paris
        .from_local_datetime(&day.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .ok_or(Error("ambiguous midnight"))?;
    let end = Paris
        .from_local_datetime(&next.and_hms_opt(0, 0, 0).unwrap())
        .single()
        .ok_or(Error("ambiguous midnight"))?;
    Ok((start.timestamp(), end.timestamp()))
}
/// Seven completed Paris days. One daily median gets one vote. Three days are required.
pub fn baseline(samples: &[HistorySample], now: i64) -> Baseline {
    let Some(utc) = DateTime::from_timestamp(now, 0) else {
        return Baseline {
            price: None,
            day_count: 0,
            source_count: 0,
        };
    };
    let today = utc.with_timezone(&Paris).date_naive();
    let Some(first) = today.checked_sub_days(Days::new(7)) else {
        return Baseline {
            price: None,
            day_count: 0,
            source_count: 0,
        };
    };
    let mut days: BTreeMap<_, Vec<UnitPrice>> = BTreeMap::new();
    let mut sources = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for s in samples {
        if !s.eligible || s.price.numerator() == 0 {
            continue;
        }
        let Some(t) = DateTime::from_timestamp(s.observed_at, 0) else {
            continue;
        };
        let day = t.with_timezone(&Paris).date_naive();
        if day >= first && day < today {
            // Duplicate presenter samples with the same identity cannot increase daily weight.
            if seen.insert((
                s.observed_at,
                s.price.numerator(),
                s.price.denominator(),
                s.source_id.clone(),
            )) {
                days.entry(day).or_default().push(s.price);
                sources.insert(s.source_id.clone());
            }
        }
    }
    let day_count = days.len();
    let price = if day_count >= 3 {
        median(days.into_values().filter_map(median).collect())
    } else {
        None
    };
    Baseline {
        price,
        day_count,
        source_count: sources.len(),
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignalRule {
    Threshold { max_unit_price: UnitPrice },
    Underpriced { discount_percent: u8 },
    Movement { change_percent: u8 },
}
#[derive(Debug, Clone)]
pub struct SignalContext<'a> {
    pub now: i64,
    pub observed_at: i64,
    pub eligible: bool,
    pub current: UnitPrice,
    pub baseline: &'a Baseline,
    pub previous: Option<UnitPrice>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub kind: String,
    pub observed_at: i64,
    pub current: UnitPrice,
    pub reference: UnitPrice,
    pub baseline_days: usize,
    pub source_count: usize,
}
// Prices are nonnegative u64 rationals. Products of two u64 fit u128. Compare scaled values without overflow.
fn scaled_le(a: UnitPrice, a_scale: u64, b: UnitPrice, b_scale: u64) -> bool {
    let left = a.numerator() as u128 * b.denominator() as u128;
    let right = b.numerator() as u128 * a.denominator() as u128;
    let g = gcd(left, right);
    let left = left / g;
    let right = right / g;
    match (
        left.checked_mul(a_scale as u128),
        right.checked_mul(b_scale as u128),
    ) {
        (Some(l), Some(r)) => l <= r,
        (None, Some(_)) => false,
        (Some(_), None) => true,
        (None, None) => {
            // Both exceed u128 only when scaling; quotient and remainder compare exactly.
            let lq = left / (b_scale as u128);
            let rq = right / (a_scale as u128);
            if lq != rq {
                lq < rq
            } else {
                (left % (b_scale as u128)) * a_scale as u128
                    <= (right % (a_scale as u128)) * b_scale as u128
            }
        }
    }
}
fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let r = a % b;
        a = b;
        b = r
    }
    a.max(1)
}
pub fn evaluate(ctx: SignalContext<'_>, rule: &SignalRule) -> Option<Signal> {
    let age = ctx.now.checked_sub(ctx.observed_at)?;
    if !ctx.eligible || !(0..=45 * 60).contains(&age) || ctx.current.numerator() == 0 {
        return None;
    }
    let (kind, reference, yes) = match rule {
        SignalRule::Threshold { max_unit_price } => (
            "threshold",
            *max_unit_price,
            max_unit_price.numerator() > 0 && ctx.current <= *max_unit_price,
        ),
        SignalRule::Underpriced { discount_percent } => {
            if *discount_percent > 100 || ctx.baseline.day_count < 3 {
                return None;
            }
            let b = ctx.baseline.price?;
            (
                "underpriced",
                b,
                b.numerator() > 0 && scaled_le(ctx.current, 100, b, 100 - *discount_percent as u64),
            )
        }
        SignalRule::Movement { change_percent } => {
            if *change_percent == 0 || *change_percent > 100 {
                return None;
            }
            let p = ctx.previous?;
            let changed = p.numerator() > 0
                && (scaled_le(ctx.current, 100, p, 100 - *change_percent as u64)
                    || scaled_le(p, 100 + *change_percent as u64, ctx.current, 100));
            ("movement", p, changed)
        }
    };
    yes.then(|| Signal {
        kind: kind.into(),
        observed_at: ctx.observed_at,
        current: ctx.current,
        reference,
        baseline_days: ctx.baseline.day_count,
        source_count: ctx.baseline.source_count,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn p(n: u64, d: u64) -> UnitPrice {
        UnitPrice::new(n, d).unwrap()
    }
    #[test]
    fn weighted_exact_bid_only() {
        let l = |n, b| Listing {
            item_key: "i1".into(),
            stack_count: n,
            buyout_total_copper: b,
            min_bid_copper: 0,
            current_bid_copper: 0,
            time_left_band: 1,
        };
        let m = snapshot(&[l(2, Some(7)), l(20, Some(80)), l(100, None)]).unwrap();
        let m = &m["i1"];
        assert_eq!(m.min_buyout, Some(p(7, 2)));
        assert_eq!(m.median_buyout, Some(p(4, 1)));
        assert_eq!(m.total_units, 122)
    }
    #[test]
    fn paris_dst() {
        let ts = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().timestamp();
        let (a, b) = paris_day_bounds(ts("2026-03-29T12:00:00Z")).unwrap();
        assert_eq!(b - a, 23 * 3600);
        let (a, b) = paris_day_bounds(ts("2026-10-25T12:00:00Z")).unwrap();
        assert_eq!(b - a, 25 * 3600)
    }
    #[test]
    fn daily_coverage_and_freshness() {
        let now = DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z")
            .unwrap()
            .timestamp();
        let samples: Vec<_> = (1..=3)
            .map(|d| HistorySample {
                observed_at: now - d * 86400,
                price: p(100, 1),
                source_id: "one".into(),
                eligible: true,
            })
            .collect();
        assert!(baseline(&samples[..2], now).price.is_none());
        let b = baseline(&samples, now);
        assert_eq!(b.price, Some(p(100, 1)));
        let rule = SignalRule::Underpriced {
            discount_percent: 30,
        };
        let ctx = |age| SignalContext {
            now,
            observed_at: now - age,
            eligible: true,
            current: p(70, 1),
            baseline: &b,
            previous: None,
        };
        assert!(evaluate(ctx(2700), &rule).is_some());
        assert!(evaluate(ctx(2701), &rule).is_none());
        assert!(evaluate(ctx(-1), &rule).is_none());
    }
    #[test]
    fn maximum_exact_scaling() {
        assert!(scaled_le(
            p(u64::MAX, u64::MAX - 1),
            100,
            p(u64::MAX, u64::MAX - 1),
            100
        ));
        assert!(!scaled_le(p(71, 1), 100, p(100, 1), 70));
    }
}

#[cfg(test)]
mod adversarial_tests {
    use super::*;
    fn p(n: u64, d: u64) -> UnitPrice {
        UnitPrice::new(n, d).unwrap()
    }
    #[test]
    fn scaled_prices_compare_overflow_paths_exactly() {
        let a = p(u64::MAX, u64::MAX - 1);
        let b = p(u64::MAX - 1, u64::MAX - 2);
        assert!(scaled_le(a, 100, b, 100));
        assert!(!scaled_le(b, 100, a, 100));
        assert!(!scaled_le(a, 100, b, 70));
        for n in 1..25 {
            for d in 1..25 {
                for n2 in 1..25 {
                    for scale in [1, 30, 70, 100, 130] {
                        assert_eq!(
                            scaled_le(p(n, d), 100, p(n2, 3), scale),
                            (n as u128 * 3 * 100) <= (n2 as u128 * d as u128 * scale as u128)
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn isolated_snapshots_and_bid_only_snapshots() {
        let listing = Listing {
            item_key: "i1".into(),
            stack_count: 5,
            buyout_total_copper: None,
            min_bid_copper: 1,
            current_bid_copper: 0,
            time_left_band: 1,
        };
        let metrics = snapshot(std::slice::from_ref(&listing)).unwrap();
        assert!(metrics["i1"].min_buyout.is_none());
        assert!(metrics["i1"].median_buyout.is_none());
        assert_eq!(metrics["i1"].total_units, 5);
        let mut invalid = listing.clone();
        invalid.stack_count = 0;
        assert!(snapshot(&[invalid]).is_err());
        let mut extreme = listing;
        extreme.stack_count = i64::MAX as u64;
        assert!(snapshot(&[extreme.clone(), extreme]).is_err());
        assert_eq!(snapshot(&[]).unwrap().len(), 0);
    }
    #[test]
    fn baseline_weights_dates_and_excludes_current_day() {
        let now = DateTime::parse_from_rfc3339("2026-10-27T12:00:00Z")
            .unwrap()
            .timestamp();
        let sample = |days: i64, price: u64, eligible: bool| HistorySample {
            observed_at: now - days * 86400,
            price: p(price, 1),
            source_id: "source".into(),
            eligible,
        };
        let mut samples = vec![
            sample(1, 100, true),
            sample(2, 200, true),
            sample(3, 300, true),
            sample(0, 1, true),
            sample(8, 1, true),
            sample(4, 1, false),
            sample(5, 0, true),
        ];
        for _ in 0..100 {
            samples.push(sample(3, 300, true));
        }
        let b = baseline(&samples, now);
        assert_eq!(b.price, Some(p(200, 1)));
        assert_eq!(b.day_count, 3);
        assert_eq!(b.source_count, 1);
    }
    #[test]
    fn threshold_movement_and_quality_gates() {
        let b = Baseline {
            price: Some(p(100, 1)),
            day_count: 3,
            source_count: 1,
        };
        let ctx = |current, eligible| SignalContext {
            now: 1000,
            observed_at: 1000,
            eligible,
            current: p(current, 1),
            baseline: &b,
            previous: Some(p(100, 1)),
        };
        assert!(evaluate(
            ctx(70, true),
            &SignalRule::Threshold {
                max_unit_price: p(70, 1)
            }
        )
        .is_some());
        assert!(evaluate(
            ctx(71, true),
            &SignalRule::Threshold {
                max_unit_price: p(70, 1)
            }
        )
        .is_none());
        assert!(evaluate(ctx(130, true), &SignalRule::Movement { change_percent: 30 }).is_some());
        assert!(evaluate(ctx(70, true), &SignalRule::Movement { change_percent: 30 }).is_some());
        assert!(evaluate(ctx(129, true), &SignalRule::Movement { change_percent: 30 }).is_none());
        assert!(evaluate(
            ctx(70, false),
            &SignalRule::Underpriced {
                discount_percent: 30
            }
        )
        .is_none());
        assert!(evaluate(
            ctx(0, true),
            &SignalRule::Threshold {
                max_unit_price: p(70, 1)
            }
        )
        .is_none());
        assert!(evaluate(
            ctx(70, true),
            &SignalRule::Underpriced {
                discount_percent: 101
            }
        )
        .is_none());
    }
}
pub mod crafting;
