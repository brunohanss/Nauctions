//! Experimental asking-price forecasts. Missing observations never become sales or zero prices.
use market_domain::UnitPrice;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const DAY: i64 = 86400;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sample {
    pub at: i64,
    pub price: UnitPrice,
}
#[derive(Clone, Debug, Serialize)]
pub struct Estimate {
    pub status: &'static str,
    pub reason: &'static str,
    pub method: &'static str,
    pub horizon_hours: i64,
    pub target_at: i64,
    pub price: Option<UnitPrice>,
    pub low: Option<UnitPrice>,
    pub high: Option<UnitPrice>,
    pub change_percent: Option<f64>,
    pub direction: &'static str,
    pub observations: usize,
    pub validation_pairs: usize,
    pub mean_absolute_log_error: Option<f64>,
    pub baseline_error: Option<f64>,
    pub range_kind: &'static str,
}
pub(crate) fn empty(now: i64, hours: i64, reason: &'static str) -> Estimate {
    Estimate {
        status: "insufficient",
        reason,
        method: "none",
        horizon_hours: hours,
        target_at: now + hours * 3600,
        price: None,
        low: None,
        high: None,
        change_percent: None,
        direction: "unknown",
        observations: 0,
        validation_pairs: 0,
        mean_absolute_log_error: None,
        baseline_error: None,
        range_kind: "unavailable",
    }
}
fn val(p: UnitPrice) -> f64 {
    p.numerator() as f64 / p.denominator() as f64
}
fn copper(p: f64) -> Option<UnitPrice> {
    // Forecasts use floating point internally; never imply exact precision beyond this bound.
    if !p.is_finite() || !(0.0001..=9_007_199_254_740_991.0).contains(&p) {
        return None;
    }
    let scale = if p <= 9_007_199_254_740_991.0 / 10000.0 {
        10000
    } else {
        1
    };
    let n = (p * scale as f64).round() as u64;
    let (mut a, mut b) = (n, scale);
    while b != 0 {
        let rem = a % b;
        a = b;
        b = rem;
    }
    UnitPrice::new(n / a, scale / a)
}
fn median(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(f64::total_cmp);
    xs[(xs.len() - 1) / 2]
}
fn hourly(samples: &[Sample], now: i64) -> Vec<Sample> {
    let mut hours = BTreeMap::<i64, Sample>::new();
    for s in samples
        .iter()
        .filter(|s| s.at >= now - 60 * DAY && s.at <= now && s.price.numerator() > 0)
    {
        // Latest real observation in each UTC hour. Repeated uploads add no weight.
        let key = s.at.div_euclid(3600);
        if hours.get(&key).is_none_or(|old| s.at > old.at) {
            hours.insert(key, s.clone());
        }
    }
    hours.into_values().collect()
}
fn predict(ss: &[Sample], model: usize) -> f64 {
    let last = ss.last().unwrap();
    let current = val(last.price).ln();
    match model {
        1 => {
            let recent: Vec<_> = ss
                .iter()
                .filter(|s| s.at >= last.at - 6 * 3600)
                .map(|s| val(s.price).ln())
                .collect();
            let old: Vec<_> = ss
                .iter()
                .filter(|s| (last.at - DAY - 3 * 3600..=last.at - DAY + 3 * 3600).contains(&s.at))
                .map(|s| val(s.price).ln())
                .collect();
            if old.is_empty() {
                current
            } else {
                let level = median(recent);
                level + (level - median(old)).clamp(-0.35, 0.35) * 0.5
            }
        }
        2 => {
            let matched: Vec<_> = ss
                .iter()
                .filter(|s| {
                    let age = last.at - s.at;
                    age >= 20 * 3600 && (age % DAY).min(DAY - age % DAY) <= 2 * 3600
                })
                .map(|s| val(s.price).ln())
                .collect();
            if matched.len() < 3 {
                current
            } else {
                median(matched)
            }
        }
        _ => current,
    }
}

pub fn rolling(samples: &[Sample], now: i64, eligible: bool) -> Estimate {
    let mut out = empty(
        now,
        24,
        "At least seven days and 48 observed hours are required.",
    );
    let ss = hourly(samples, now);
    out.observations = ss.len();
    if !eligible || ss.last().is_none_or(|s| now - s.at > 2700) {
        out.reason = "A fresh complete full scan with a positive median is required.";
        return out;
    }
    // Freshness uses the original scan; the hour representative may be older.
    if ss.len() < 48 || ss.last().unwrap().at - ss[0].at < 7 * DAY {
        return out;
    }
    let mut errors: Vec<[f64; 3]> = vec![];
    let mut last_origin = i64::MIN / 2;
    for (i, origin) in ss.iter().enumerate() {
        if i < 24 || origin.at - ss[0].at < 3 * DAY || origin.at - last_origin < 6 * 3600 {
            continue;
        }
        let target = origin.at + DAY;
        let actual = ss[i + 1..]
            .iter()
            .filter(|s| (s.at - target).abs() <= 3600)
            .min_by_key(|s| ((s.at - target).abs(), s.at));
        if let Some(actual) = actual {
            let y = val(actual.price).ln();
            errors.push([
                y - predict(&ss[..=i], 0),
                y - predict(&ss[..=i], 1),
                y - predict(&ss[..=i], 2),
            ]);
            last_origin = origin.at;
        }
    }
    out.validation_pairs = errors.len();
    if errors.len() < 36 {
        out.reason = "At least 36 historical 24-hour comparisons are required.";
        return out;
    }
    // Select, compare with baseline, then evaluate on a third untouched period.
    // Keep a 24h embargo between periods so their target windows do not overlap.
    let split = errors.len() / 3;
    let mae = |rows: &[[f64; 3]], model: usize| {
        rows.iter().map(|r| r[model].abs()).sum::<f64>() / rows.len() as f64
    };
    let candidate = (0..3)
        .min_by(|&a, &b| mae(&errors[..split], a).total_cmp(&mae(&errors[..split], b)))
        .unwrap();
    let promotion = &errors[split + 4..2 * split];
    let model = if mae(promotion, candidate) < mae(promotion, 0) * 0.95 {
        candidate
    } else {
        0
    };
    let held = &errors[2 * split + 4..];
    if model != 0 && mae(held, model) > mae(held, 0) {
        out.reason="The selected model failed the untouched later-period baseline check. No estimate is published.";
        out.method = ["last_price", "damped_trend", "same_hour_median"][model];
        out.validation_pairs = held.len();
        out.mean_absolute_log_error = Some(mae(held, model));
        out.baseline_error = Some(mae(held, 0));
        return out;
    }
    let center = predict(&ss, model);
    let mut residuals: Vec<_> = held.iter().map(|r| r[model].abs()).collect();
    residuals.sort_by(f64::total_cmp);
    let width = residuals
        [((residuals.len() as f64 * 0.9).ceil() as usize - 1).min(residuals.len() - 1)]
    .max(0.05);
    fill(
        &mut out,
        val(ss.last().unwrap().price),
        center.exp(),
        (center - width).exp(),
        (center + width).exp(),
    );
    out.status = if model == 0 {
        "baseline"
    } else {
        "experimental"
    };
    out.reason = if model == 0 {
        "Other models did not improve sufficiently on the unchanged-price baseline."
    } else {
        "Experimental model improved on the baseline in later historical comparisons."
    };
    out.method = ["last_price", "damped_trend", "same_hour_median"][model];
    out.validation_pairs = held.len();
    out.mean_absolute_log_error = Some(mae(held, model));
    out.baseline_error = Some(mae(held, 0));
    out.range_kind = "historical_error_range_with_0_05_log_floor_not_probability";
    if out.price.is_none() || out.low.is_none() || out.high.is_none() {
        return empty(now, 24, "Estimate exceeds the supported price range.");
    }
    out
}
pub(crate) fn fill(out: &mut Estimate, anchor: f64, center: f64, low: f64, high: f64) {
    out.price = copper(center);
    out.low = copper(low.max(0.0001));
    out.high = copper(high);
    let change = (center / anchor - 1.0) * 100.0;
    out.change_percent = Some(change);
    out.direction = if change > 5.0 {
        "up"
    } else if change < -5.0 {
        "down"
    } else {
        "stable"
    };
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BetaRun {
    pub run_id: String,
    pub realm: String,
    pub house: String,
    pub build: String,
    pub notes: String,
    pub started_at: i64,
    pub samples: Vec<BetaSample>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BetaSample {
    pub item_key: String,
    pub elapsed_hours: u32,
    pub price: UnitPrice,
}
pub fn launch(
    runs: &[BetaRun],
    key: &str,
    live_started_at: Option<i64>,
    anchor: Option<UnitPrice>,
    now: i64,
) -> Vec<Estimate> {
    [24,72,168].into_iter().map(|hours| {
        let mut out = empty(now, hours, "Import comparable beta runs and set the live launch time.");
        let (Some(start), Some(anchor)) = (live_started_at, anchor) else { return out; };
        if start > now { out.reason = "The live launch time is in the future."; return out; }
        let age = (now-start) as f64 / 3600.0;
        let mut ratios = vec![];
        for run in runs {
            let near = |hour: f64| run.samples.iter().filter(|s| s.item_key == key && s.price.numerator() > 0 && (f64::from(s.elapsed_hours)-hour).abs() <= 3.0)
                .min_by(|a,b| (f64::from(a.elapsed_hours)-hour).abs().total_cmp(&(f64::from(b.elapsed_hours)-hour).abs()));
            if let (Some(a),Some(b)) = (near(age), near(age+hours as f64)) { ratios.push((val(b.price)/val(a.price)).ln()); }
        }
        if ratios.is_empty() { out.reason = "No beta run covers this item at both launch ages."; return out; }
        let center = median(ratios.clone());
        let low = ratios.iter().copied().fold(f64::INFINITY, f64::min) - 0.25;
        let high = ratios.iter().copied().fold(f64::NEG_INFINITY, f64::max) + 0.25;
        fill(&mut out, val(anchor), val(anchor)*center.exp(), val(anchor)*low.exp(), val(anchor)*high.exp());
        out.status = "beta_based"; out.method = "launch_age_relative_change";
        out.reason = "Beta scenario anchored to the fresh live median. Beta conditions can differ from launch.";
        out.observations = ratios.len(); out.range_kind = "beta_scenario_range_not_probability";
        if out.price.is_none() || out.low.is_none() || out.high.is_none() { return empty(now, hours, "Estimate exceeds the supported price range."); }
        out
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn history() -> Vec<Sample> {
        (0..24 * 20)
            .map(|h| Sample {
                at: 1_700_000_000 + h * 3600,
                price: UnitPrice::new(1000, 1).unwrap(),
            })
            .collect()
    }
    #[test]
    fn stable_market_uses_baseline_and_positive_range() {
        let ss = history();
        let f = rolling(&ss, ss.last().unwrap().at, true);
        assert_eq!(f.status, "baseline");
        assert_eq!(val(f.price.unwrap()), 1000.0);
        assert!(f.low.unwrap() < f.price.unwrap());
        assert!(f.high.unwrap() > f.price.unwrap());
        assert!(f.validation_pairs >= 8);
    }
    #[test]
    fn fractional_unit_prices_keep_their_scale_and_ordered_range() {
        let mut ss = history();
        for s in &mut ss {
            s.price = UnitPrice::new(1, 200).unwrap();
        }
        let f = rolling(&ss, ss.last().unwrap().at, true);
        assert_eq!(f.price.unwrap(), UnitPrice::new(1, 200).unwrap());
        assert!(f.low.unwrap() <= f.price.unwrap());
        assert!(f.high.unwrap() >= f.price.unwrap());
    }
    #[test]
    fn future_data_and_duplicates_do_not_change_forecast() {
        let mut ss = history();
        let now = ss.last().unwrap().at;
        let before = serde_json::to_value(rolling(&ss, now, true)).unwrap();
        ss.extend(ss.clone());
        ss.push(Sample {
            at: now + 3600,
            price: UnitPrice::new(999999, 1).unwrap(),
        });
        assert_eq!(
            before,
            serde_json::to_value(rolling(&ss, now, true)).unwrap()
        );
    }
    #[test]
    fn a_fresh_late_hour_scan_is_not_hidden_by_an_older_hour_representative() {
        let mut ss = history();
        let now = ss.last().unwrap().at + 2750;
        ss.push(Sample {
            at: now,
            price: UnitPrice::new(1000, 1).unwrap(),
        });
        assert_eq!(rolling(&ss, now, true).status, "baseline");
    }
    #[test]
    fn stale_sparse_and_ineligible_data_produce_no_estimate() {
        let ss = history();
        let now = ss.last().unwrap().at;
        for f in [
            rolling(&ss, now, false),
            rolling(&ss, now + 2800, true),
            rolling(&ss[..20], ss[19].at, true),
        ] {
            assert!(f.price.is_none());
        }
    }
    #[test]
    fn beta_ratios_are_relative_and_require_both_ages() {
        let run = BetaRun {
            run_id: "reset1".into(),
            realm: "beta".into(),
            house: "Alliance".into(),
            build: "test".into(),
            notes: "reset".into(),
            started_at: 0,
            samples: vec![
                BetaSample {
                    item_key: "i1".into(),
                    elapsed_hours: 0,
                    price: UnitPrice::new(10, 1).unwrap(),
                },
                BetaSample {
                    item_key: "i1".into(),
                    elapsed_hours: 24,
                    price: UnitPrice::new(20, 1).unwrap(),
                },
            ],
        };
        let fs = launch(&[run], "i1", Some(100), UnitPrice::new(100, 1), 100);
        assert_eq!(val(fs[0].price.unwrap()), 200.0);
        assert!(fs[1].price.is_none());
        assert!(launch(&[], "i1", None, None, 100)[0].price.is_none());
    }
}
