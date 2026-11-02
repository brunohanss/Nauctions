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
fn empty(now: i64, hours: i64, reason: &'static str) -> Estimate {
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
        // For equal timestamps, callers order by received_at and scan id; keep the last.
        let key = s.at.div_euclid(3600);
        if hours.get(&key).is_none_or(|old| s.at >= old.at) {
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

// Count dates in the market's calendar. Same-hour repeats and clustered scans
// cannot substitute for observations at different times of day.
fn covered_days(ss: &[Sample]) -> usize {
    let mut dates = BTreeMap::new();
    for sample in ss {
        if let Some(at) = chrono::DateTime::from_timestamp(sample.at, 0) {
            let date = at.with_timezone(&chrono_tz::Europe::Paris).date_naive();
            let bounds = dates.entry(date).or_insert((sample.at, sample.at));
            bounds.1 = sample.at;
        }
    }
    dates
        .values()
        .filter(|(first, last)| last - first >= 4 * 3600)
        .count()
}
fn error_width(mut errors: Vec<f64>) -> f64 {
    errors.sort_by(f64::total_cmp);
    errors[((errors.len() as f64 * 0.9).ceil() as usize - 1).min(errors.len() - 1)].max(0.05)
}
// The fixed last-price method needs no model fitting. Its error evidence uses
// actual next-day observations, with a wider tolerance for manual scan times.
fn basic(ss: &[Sample], now: i64, mut out: Estimate) -> Estimate {
    let mut errors = Vec::new();
    let mut last_origin = i64::MIN / 2;
    for (i, origin) in ss.iter().enumerate() {
        if origin.at - last_origin < 6 * 3600 {
            continue;
        }
        let target = origin.at + DAY;
        if let Some(actual) = ss[i + 1..]
            .iter()
            .filter(|s| (s.at - target).abs() <= 3 * 3600)
            .min_by_key(|s| ((s.at - target).abs(), s.at))
        {
            errors.push((val(actual.price) / val(origin.price)).ln().abs());
            last_origin = origin.at;
        }
    }
    let current = val(ss.last().unwrap().price);
    out.status = "baseline";
    out.method = "last_price";
    out.reason = "Basic unchanged-price estimate from at least three days with scans at different times. Limited history; trend models do not yet have enough later validation.";
    out.validation_pairs = errors.len();
    out.price = copper(current);
    out.change_percent = Some(0.0);
    out.direction = "stable";
    out.range_kind = "limited_history_no_comparisons";
    if !errors.is_empty() {
        let error = errors.iter().sum::<f64>() / errors.len() as f64;
        let width = error_width(errors);
        fill(
            &mut out,
            current,
            current,
            current * (-width).exp(),
            current * width.exp(),
        );
        out.mean_absolute_log_error = Some(error);
        out.baseline_error = Some(error);
        out.range_kind = "limited_history_error_range_not_probability";
    }
    if out.price.is_none()
        || (out.range_kind == "limited_history_error_range_not_probability"
            && (out.low.is_none() || out.high.is_none()))
    {
        return empty(now, 24, "Estimate exceeds the supported price range.");
    }
    out
}

// Estimate a daily trend from matched real observations over the latest four days.
// The fixed damping rule is provisional; it does not need a model-selection window.
fn daily_pairs(ss: &[Sample]) -> Vec<(usize, usize, f64)> {
    let mut pairs = Vec::new();
    let mut last_origin = i64::MIN / 2;
    let cutoff = ss.last().unwrap().at - 4 * DAY;
    for (i, origin) in ss.iter().enumerate() {
        if origin.at < cutoff || origin.at - last_origin < 6 * 3600 {
            continue;
        }
        let target = origin.at + DAY;
        if let Some((j, actual)) = ss
            .iter()
            .enumerate()
            .skip(i + 1)
            .filter(|(_, s)| (s.at - target).abs() <= 3 * 3600)
            .min_by_key(|(_, s)| ((s.at - target).abs(), s.at))
        {
            pairs.push((i, j, (val(actual.price) / val(origin.price)).ln()));
            last_origin = origin.at;
        }
    }
    pairs
}
fn early_prediction(ss: &[Sample]) -> Option<f64> {
    let changes = daily_pairs(ss);
    if changes.len() < 3 {
        return None;
    }
    let change = median(changes.iter().map(|(_, _, change)| *change).collect());
    Some(val(ss.last().unwrap().price).ln() + change.clamp(-0.35, 0.35) * 0.5)
}
fn early(ss: &[Sample], now: i64, mut out: Estimate) -> Estimate {
    let Some(center) = early_prediction(ss) else {
        return basic(ss, now, out);
    };
    let mut errors = Vec::new();
    let mut baseline = Vec::new();
    // Each historical prediction sees only samples available at that origin.
    for (origin, target, _) in daily_pairs(ss) {
        if let Some(predicted) = early_prediction(&ss[..=origin]) {
            let actual = val(ss[target].price).ln();
            errors.push((actual - predicted).abs());
            baseline.push((actual - val(ss[origin].price).ln()).abs());
        }
    }
    let current = val(ss.last().unwrap().price);
    out.status = "experimental";
    out.method = "damped_daily_change";
    out.reason = "Provisional trend from recent matched next-day prices. Half of the median daily log-price change is applied to the latest price. Limited history; improvement over the unchanged-price baseline is not established.";
    out.price = copper(center.exp());
    out.change_percent = Some((center.exp() / current - 1.0) * 100.0);
    out.direction = if out.change_percent.unwrap() > 5.0 {
        "up"
    } else if out.change_percent.unwrap() < -5.0 {
        "down"
    } else {
        "stable"
    };
    out.validation_pairs = errors.len();
    out.mean_absolute_log_error = None;
    out.baseline_error = None;
    out.range_kind = "limited_history_no_comparisons";
    if !errors.is_empty() {
        out.mean_absolute_log_error = Some(errors.iter().sum::<f64>() / errors.len() as f64);
        out.baseline_error = Some(baseline.iter().sum::<f64>() / baseline.len() as f64);
        let width = error_width(errors).max(0.10);
        fill(
            &mut out,
            current,
            center.exp(),
            (center - width).exp(),
            (center + width).exp(),
        );
        out.range_kind = "limited_history_error_range_not_probability";
    }
    if out.price.is_none()
        || (out.range_kind == "limited_history_error_range_not_probability"
            && (out.low.is_none() || out.high.is_none()))
    {
        return empty(now, 24, "Estimate exceeds the supported price range.");
    }
    out
}

/// Observed historical outcomes, not a calibrated forecast probability.
#[derive(Clone, Debug, Serialize)]
pub struct RiseFrequency {
    pub horizon_hours: i64,
    pub status: &'static str,
    pub reason: &'static str,
    pub percent: Option<f64>,
    pub rises: usize,
    pub comparisons: usize,
    pub method: &'static str,
}
pub fn rise_frequencies(samples: &[Sample], now: i64, eligible: bool) -> Vec<RiseFrequency> {
    use chrono::Timelike;
    let ss = hourly(samples, now);
    let local_time = |at| {
        chrono::DateTime::from_timestamp(at, 0).map(|t| {
            t.with_timezone(&chrono_tz::Europe::Paris)
                .num_seconds_from_midnight() as i64
        })
    };
    [12, 24].into_iter().map(|hours| {
        let mut out = RiseFrequency { horizon_hours: hours, status: "insufficient",
            reason: "At least 10 non-overlapping historical comparisons near the current local hour are required.",
            percent: None, rises: 0, comparisons: 0,
            method: "historical_time_matched_frequency_not_probability" };
        let Some(last) = ss.last() else { return out; };
        if !eligible || now - last.at > 2700 {
            out.reason = "A fresh complete full scan with a positive median is required.";
            return out;
        }
        let mut previous_end = i64::MIN;
        for (i, origin) in ss.iter().enumerate() {
            let Some(hour) = local_time(origin.at) else { continue; };
            let distance = (hour - local_time(last.at).unwrap()).abs();
            if origin.at <= previous_end || distance.min(DAY - distance) > 3600 { continue; }
            let target = origin.at + hours * 3600;
            if let Some(actual) = ss[i + 1..].iter()
                .filter(|s| (s.at - target).abs() <= 3600)
                .min_by_key(|s| ((s.at - target).abs(), s.at)) {
                out.comparisons += 1;
                out.rises += usize::from(actual.price > origin.price);
                previous_end = actual.at;
            }
        }
        if out.comparisons >= 10 {
            out.status = "available";
            out.reason = "Share of historical asking-price medians that rose. Origins match the latest Europe/Paris hour within one hour; targets match the horizon within one hour. Windows do not overlap. Unchanged prices count as no rise. This is not a calibrated chance of a future rise.";
            out.percent = Some(100.0 * out.rises as f64 / out.comparisons as f64);
        }
        out
    }).collect()
}

pub fn rolling(samples: &[Sample], now: i64, eligible: bool) -> Estimate {
    let mut out = empty(
        now,
        24,
        "At least three days with two observations at least four hours apart per day are required.",
    );
    let ss = hourly(samples, now);
    out.observations = ss.len();
    if !eligible || ss.last().is_none_or(|s| now - s.at > 2700) {
        out.reason = "A fresh complete full scan with a positive median is required.";
        return out;
    }
    // Three observed calendar dates can be covered before 72 hours elapse.
    if ss.len() < 6 || covered_days(&ss) < 3 {
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
        return early(&ss, now, out);
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
fn fill(out: &mut Estimate, anchor: f64, center: f64, low: f64, high: f64) {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_timestamp_uses_last_canonical_scan_for_hourly_price() {
        let ss = vec![
            Sample {
                at: 1_700_000_000,
                price: UnitPrice::new(1000, 1).unwrap(),
            },
            Sample {
                at: 1_700_000_000,
                price: UnitPrice::new(1200, 1).unwrap(),
            },
        ];
        let result = hourly(&ss, ss[1].at);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].price, ss[1].price);
    }
    #[test]
    fn rise_frequency_counts_non_overlapping_real_outcomes_and_ties() {
        let mut ss = history();
        for (i, s) in ss.iter_mut().enumerate() {
            s.price = UnitPrice::new(1000 + i as u64, 1).unwrap();
        }
        let now = ss.last().unwrap().at;
        for result in rise_frequencies(&ss, now, true) {
            assert_eq!(result.percent, Some(100.0));
            assert!(result.comparisons >= 10 && result.comparisons <= 20);
            assert_eq!(result.rises, result.comparisons);
        }
        for s in &mut ss {
            s.price = UnitPrice::new(1000, 1).unwrap();
        }
        assert!(rise_frequencies(&ss, now, true)
            .iter()
            .all(|f| f.percent == Some(0.0)));
    }
    #[test]
    fn rise_frequency_suppresses_sparse_stale_ineligible_and_unmatched_history() {
        let ss = history();
        let now = ss.last().unwrap().at;
        for frequencies in [
            rise_frequencies(&ss, now, false),
            rise_frequencies(&ss, now + 2701, true),
            rise_frequencies(&ss[..100], ss[99].at, true),
        ] {
            assert!(frequencies.iter().all(|f| f.percent.is_none()));
        }
        let once_daily: Vec<_> = ss.iter().step_by(24).cloned().collect();
        let result = rise_frequencies(&once_daily, once_daily.last().unwrap().at, true);
        assert_eq!(result[0].comparisons, 0);
        assert!(result[0].percent.is_none());
    }
    #[test]
    fn rise_frequency_ignores_duplicates_and_future_samples() {
        let mut ss = history();
        let now = ss.last().unwrap().at;
        let before = serde_json::to_value(rise_frequencies(&ss, now, true)).unwrap();
        ss.extend(ss.clone());
        ss.push(Sample {
            at: now + 3600,
            price: UnitPrice::new(999999, 1).unwrap(),
        });
        assert_eq!(
            before,
            serde_json::to_value(rise_frequencies(&ss, now, true)).unwrap()
        );
    }
    fn history() -> Vec<Sample> {
        (0..24 * 20)
            .map(|h| Sample {
                at: 1_700_000_000 + h * 3600,
                price: UnitPrice::new(1000, 1).unwrap(),
            })
            .collect()
    }
    fn manual_history(hours: &[i64]) -> Vec<Sample> {
        let start = chrono::DateTime::parse_from_rfc3339("2026-10-20T00:00:00+02:00")
            .unwrap()
            .timestamp();
        (0..7)
            .flat_map(|day| {
                hours.iter().map(move |hour| Sample {
                    at: start + day * DAY + hour * 3600,
                    price: UnitPrice::new(1000 + day as u64 * 20, 1).unwrap(),
                })
            })
            .collect()
    }
    #[test]
    fn two_or_three_manual_scans_on_seven_dates_produce_a_provisional_trend() {
        for hours in [vec![8, 20], vec![8, 12, 20]] {
            let ss = manual_history(&hours);
            let f = rolling(&ss, ss.last().unwrap().at, true);
            assert_eq!(f.status, "experimental");
            assert!(f.price.unwrap() > ss.last().unwrap().price);
            assert_eq!(f.method, "damped_daily_change");
            assert!(f.reason.contains("Limited history"));
            assert_eq!(f.range_kind, "limited_history_error_range_not_probability");
            assert!(f.validation_pairs >= 1);
            assert!(f.low.unwrap() < f.price.unwrap());
            assert!(f.high.unwrap() > f.price.unwrap());
            assert_eq!(f.observations, 7 * hours.len());
        }
    }
    #[test]
    fn three_dates_are_enough_for_rising_falling_and_stable_provisional_trends() {
        for hours in [vec![8, 20], vec![8, 12, 20]] {
            for direction in [-1, 0, 1] {
                let mut ss = manual_history(&hours);
                ss.truncate(3 * hours.len());
                for (i, sample) in ss.iter_mut().enumerate() {
                    let day = i / hours.len();
                    sample.price =
                        UnitPrice::new((1000_i64 + direction * day as i64 * 200) as u64, 1)
                            .unwrap();
                }
                let f = rolling(&ss, ss.last().unwrap().at, true);
                assert_eq!(f.status, "experimental");
                assert_eq!(f.method, "damped_daily_change");
                match direction {
                    -1 => assert!(f.price.unwrap() < ss.last().unwrap().price),
                    0 => assert_eq!(f.price, Some(ss.last().unwrap().price)),
                    _ => assert!(f.price.unwrap() > ss.last().unwrap().price),
                }
                assert_eq!(f.validation_pairs, 0);
                assert!(f.mean_absolute_log_error.is_none());
                assert!(f.low.is_none() && f.high.is_none());
            }
        }
    }
    #[test]
    fn provisional_trends_get_causal_error_checks_by_day_four_and_limit_extreme_changes() {
        let mut ss = manual_history(&[8, 20]);
        ss.truncate(8);
        for (i, sample) in ss.iter_mut().enumerate() {
            sample.price = UnitPrice::new(1000 * 10_u64.pow((i / 2) as u32), 1).unwrap();
        }
        let f = rolling(&ss, ss.last().unwrap().at, true);
        assert!(f.change_percent.unwrap() > 0.0 && f.change_percent.unwrap() < 20.0);
        assert!(f.validation_pairs > 0);
        assert!(f.mean_absolute_log_error.is_some());
        assert!(f.low.unwrap() < f.price.unwrap() && f.high.unwrap() > f.price.unwrap());
    }
    #[test]
    fn single_daily_scans_clusters_and_duplicates_do_not_satisfy_daily_coverage() {
        for hours in [vec![8], vec![8, 9, 10]] {
            let mut ss = manual_history(&hours);
            ss.extend(ss.clone());
            let f = rolling(&ss, ss.last().unwrap().at, true);
            assert_eq!(f.status, "insufficient");
            assert!(f.price.is_none());
        }
        let ss = manual_history(&[8, 20]);
        assert!(rolling(&ss[..4], ss[3].at, true).price.is_none());
    }
    #[test]
    fn basic_estimate_does_not_invent_error_evidence_when_scan_times_do_not_match() {
        let mut ss = manual_history(&[4, 8]);
        for (i, sample) in ss.iter_mut().enumerate() {
            if (i / 2) % 2 == 1 {
                sample.at += 12 * 3600;
            }
        }
        let f = rolling(&ss, ss.last().unwrap().at, true);
        assert_eq!(f.status, "baseline");
        assert!(f.price.is_some());
        assert_eq!(f.validation_pairs, 0);
        assert!(f.low.is_none() && f.high.is_none());
        assert!(f.mean_absolute_log_error.is_none());
    }
    #[test]
    fn manual_history_still_requires_a_fresh_eligible_scan() {
        let ss = manual_history(&[8, 20]);
        let now = ss.last().unwrap().at;
        assert!(rolling(&ss, now, false).price.is_none());
        assert!(rolling(&ss, now + 2701, true).price.is_none());
        let original = serde_json::to_value(rolling(&ss, now, true)).unwrap();
        let mut repeated = ss.clone();
        repeated.extend(ss.clone());
        repeated.push(Sample {
            at: now + 3600,
            price: UnitPrice::new(99999, 1).unwrap(),
        });
        assert_eq!(
            original,
            serde_json::to_value(rolling(&repeated, now, true)).unwrap()
        );
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
}
