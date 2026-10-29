//! Exploratory asking-price patterns. These are research candidates, not sale forecasts.
use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Europe::Paris;
use market_domain::UnitPrice;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, serde::Deserialize)]
pub struct Sample {
    pub at: i64,
    pub price: UnitPrice,
    pub quantity: u64,
}
#[derive(Clone, Serialize)]
pub struct Cell {
    pub weekday: u32,
    pub hour: u32,
    pub price: UnitPrice,
    pub quantity: u64,
    pub observations: usize,
    pub weeks: usize,
}
#[derive(Serialize)]
pub struct HistoricalPair {
    pub entry_at: i64,
    pub exit_at: i64,
    pub entry_price: UnitPrice,
    pub exit_price: UnitPrice,
    pub later_check: bool,
}
#[derive(Serialize)]
pub struct Pattern {
    pub detected: bool,
    pub strategy: &'static str,
    pub status: &'static str,
    pub action: &'static str,
    pub reason: &'static str,
    pub target: Option<UnitPrice>,
    pub entry_reference: Option<UnitPrice>,
    pub conservative_target: Option<UnitPrice>,
    pub holding_hours: Option<u32>,
    pub holding_range_hours: Option<[u32; 2]>,
    pub historical_pairs: Vec<HistoricalPair>,
    pub pairs: usize,
    pub training_pairs: usize,
    pub validation_pairs: usize,
    pub validation_positive: usize,
    pub historical_gross_change: Option<f64>,
    pub coverage_days: usize,
    pub observed_weeks: usize,
}
fn mid<T: Ord + Copy>(mut values: Vec<T>) -> Option<T> {
    values.sort();
    values.get(values.len().saturating_sub(1) / 2).copied()
}
fn value(p: UnitPrice) -> f64 {
    p.numerator() as f64 / p.denominator() as f64
}
fn change(a: UnitPrice, b: UnitPrice) -> f64 {
    value(b) / value(a) - 1.0
}
fn local(at: i64) -> Option<DateTime<chrono_tz::Tz>> {
    DateTime::from_timestamp(at, 0).map(|d| d.with_timezone(&Paris))
}
// One vote per local calendar hour. Duplicate presenters never add sample weight.
fn hourly(samples: &[Sample], now: i64) -> BTreeMap<i64, Sample> {
    let mut groups: BTreeMap<i64, Vec<&Sample>> = BTreeMap::new();
    for s in samples
        .iter()
        .filter(|s| s.at <= now && s.at >= now - 90 * 86400 && s.price.numerator() > 0)
    {
        if let Some(d) = local(s.at) {
            let key = i64::from(d.num_days_from_ce()) * 24 + i64::from(d.hour());
            groups.entry(key).or_default().push(s);
        }
    }
    groups
        .into_iter()
        .map(|(key, mut ss)| {
            // Select a real scan, not a synthetic timestamp/price combination.
            // A burst cannot move an hour's representative by repetition.
            ss.sort_by_key(|s| {
                let d = local(s.at).unwrap();
                (
                    (i64::from(d.minute() * 60 + d.second()) - 1800).abs(),
                    s.at,
                    s.price,
                    s.quantity,
                )
            });
            let representative = ss[0];
            (key, representative.clone())
        })
        .collect()
}
pub fn heatmap(samples: &[Sample], now: i64) -> Vec<Cell> {
    let mut groups: BTreeMap<(u32, u32), Vec<Sample>> = BTreeMap::new();
    for s in hourly(samples, now).into_values() {
        let d = local(s.at).unwrap();
        groups
            .entry((d.weekday().num_days_from_monday(), d.hour()))
            .or_default()
            .push(s);
    }
    groups
        .into_iter()
        .map(|((weekday, hour), ss)| Cell {
            weekday,
            hour,
            price: mid(ss.iter().map(|s| s.price).collect()).unwrap(),
            quantity: mid(ss.iter().map(|s| s.quantity).collect()).unwrap(),
            observations: ss.len(),
            weeks: ss
                .iter()
                .map(|s| {
                    let w = local(s.at).unwrap().iso_week();
                    (w.year(), w.week())
                })
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
        })
        .collect()
}

pub fn analyze(
    samples: &[Sample],
    now: i64,
    current: Option<UnitPrice>,
    fresh: bool,
) -> Vec<Pattern> {
    let hours = hourly(samples, now);
    let days = hours
        .keys()
        .map(|h| h.div_euclid(24))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let weeks = hours
        .values()
        .map(|s| {
            let w = local(s.at).unwrap().iso_week();
            (w.year(), w.week())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let empty = |strategy| Pattern {
        detected: false,
        strategy,
        status: "insufficient",
        action: "Collect history",
        reason: "Not enough comparable entry and exit observations.",
        target: None,
        entry_reference: None,
        conservative_target: None,
        holding_hours: None,
        holding_range_hours: None,
        historical_pairs: vec![],
        pairs: 0,
        training_pairs: 0,
        validation_pairs: 0,
        validation_positive: 0,
        historical_gross_change: None,
        coverage_days: days,
        observed_weeks: weeks,
    };
    let Some(clock) = local(now) else {
        return vec![];
    };
    let today = i64::from(clock.num_days_from_ce());
    // One entry per local date, near the current local time. Missing hours
    // remain missing. Price must never decide which observation is matched.
    let clock_seconds = i64::from(clock.num_seconds_from_midnight());
    let mut entries: BTreeMap<i64, &Sample> = BTreeMap::new();
    for s in hours.values() {
        let d = local(s.at).unwrap();
        let day = i64::from(d.num_days_from_ce());
        let distance = (i64::from(d.num_seconds_from_midnight()) - clock_seconds).abs();
        if day >= today || distance > 2 * 3600 {
            continue;
        }
        let rank = |s: &Sample| {
            (
                (i64::from(local(s.at).unwrap().num_seconds_from_midnight()) - clock_seconds).abs(),
                s.at,
            )
        };
        if entries.get(&day).is_none_or(|old| rank(s) < rank(old)) {
            entries.insert(day, s);
        }
    }
    let mut chronological: Vec<_> = hours.values().collect();
    chronological.sort_by_key(|s| s.at);
    let mut out = vec![];
    for weekly in [true, false] {
        let mut result = empty(if weekly {
            "weekly_cycle"
        } else {
            "intraday_recovery"
        });
        // Split by calendar time before selecting an exit. Validation cannot choose the model.
        let first_day = hours
            .keys()
            .next()
            .map(|h| h.div_euclid(24))
            .unwrap_or(today);
        let split = first_day + (today - first_day) * 2 / 3;
        // Entry date, actual entry/exit scans. UTC timestamps preserve elapsed
        // holding time across DST. Reuse neither entries nor exits per model.
        type PricePair<'a> = (i64, &'a Sample, &'a Sample);
        let mut best: Option<(f64, u32, Vec<PricePair>)> = None;
        for delay in 1..=if weekly { 41 } else { 5 } {
            let target_seconds = i64::from(delay) * 4 * 3600;
            let tolerance = if target_seconds <= 24 * 3600 {
                3600
            } else {
                3 * 3600
            };
            let mut used_exits = std::collections::BTreeSet::new();
            let pairs: Vec<_> = entries
                .iter()
                .filter_map(|(&day, &entry)| {
                    if weekly && (today - day).rem_euclid(7) != 0 {
                        return None;
                    }
                    let target = entry.at + target_seconds;
                    let start = chronological.partition_point(|s| s.at < target - tolerance);
                    let exit = chronological[start..]
                        .iter()
                        .copied()
                        .take_while(|s| s.at <= target + tolerance)
                        .filter(|s| {
                            s.at > entry.at
                                && s.at - entry.at <= if weekly { 164 * 3600 } else { 24 * 3600 }
                                && i64::from(local(s.at).unwrap().num_days_from_ce()) < today
                                && !used_exits.contains(&s.at)
                        })
                        .min_by_key(|s| ((s.at - target).abs(), s.at))?;
                    used_exits.insert(exit.at);
                    Some((day, entry, exit))
                })
                .collect();
            let mut train: Vec<_> = pairs
                .iter()
                .filter(|(day, _, exit)| {
                    *day < split && i64::from(local(exit.at).unwrap().num_days_from_ce()) < split
                })
                .map(|(_, a, b)| change(a.price, b.price))
                .collect();
            if train.len() < 2 {
                continue;
            }
            train.sort_by(f64::total_cmp);
            let gain = train[(train.len() - 1) / 2];
            if best.as_ref().is_none_or(|(score, _, _)| gain > *score) {
                best = Some((gain, delay, pairs));
            }
        }
        if let Some((gain, _delay, pairs)) = best {
            result.detected = gain > 0.10;
            let train: Vec<_> = pairs
                .iter()
                .filter(|(day, _, exit)| {
                    *day < split && i64::from(local(exit.at).unwrap().num_days_from_ce()) < split
                })
                .collect();
            let validation: Vec<_> = pairs.iter().filter(|(day, _, _)| *day >= split).collect();
            result.pairs = train.len() + validation.len();
            result.training_pairs = train.len();
            result.validation_pairs = validation.len();
            result.validation_positive = validation
                .iter()
                .filter(|(_, a, b)| change(a.price, b.price) > 0.10)
                .count();
            result.historical_gross_change = Some(gain * 100.0);
            result.holding_hours = mid(train
                .iter()
                .map(|(_, a, b)| ((b.at - a.at + 1800) / 3600) as u32)
                .collect());
            let durations: Vec<_> = train.iter().map(|(_, a, b)| b.at - a.at).collect();
            result.holding_range_hours = durations
                .iter()
                .min()
                .zip(durations.iter().max())
                .map(|(low, high)| [(low / 3600) as u32, ((high + 3599) / 3600) as u32]);
            // Include every usable pair (including failed later checks). The UI
            // must not pick only historical winners to illustrate the strategy.
            result.historical_pairs = train
                .iter()
                .chain(validation.iter())
                .map(|(day, a, b)| HistoricalPair {
                    entry_at: a.at,
                    exit_at: b.at,
                    entry_price: a.price,
                    exit_price: b.price,
                    later_check: *day >= split,
                })
                .collect();
            // Targets use training prices only. Validation reports subsequent price behavior.
            result.target = mid(train.iter().map(|(_, _, b)| b.price).collect());
            result.conservative_target = train.iter().map(|(_, _, b)| b.price).min();
            let entry = mid(train.iter().map(|(_, a, _)| a.price).collect());
            result.entry_reference = entry;
            let validated = gain > 0.10
                && validation.len() >= 2
                && result.validation_positive * 4 >= validation.len() * 3
                && (!weekly || result.pairs >= 4)
                && days >= if weekly { 28 } else { 7 };
            result.status = if validated { "watch" } else { "preliminary" };
            result.action = if !fresh {
                "Refresh scan"
            } else if validated && current.zip(entry).is_some_and(|(c, e)| c <= e) {
                "Review entry"
            } else {
                "Wait"
            };
            result.reason = if !fresh {
                "Current evidence is stale, incomplete, or has no buyout. Refresh before entry."
            } else if validated {
                "The selected exit passed a later price-only check. Liquidity and net profit still need review."
            } else {
                "Exploratory pattern. Later validation or historical coverage is insufficient or unstable."
            };
        }
        out.push(result);
    }
    let mut dump = empty("market_dump");
    let daily: BTreeMap<i64, (UnitPrice, u64)> = {
        let mut grouped: BTreeMap<i64, Vec<&Sample>> = BTreeMap::new();
        for (h, s) in &hours {
            let day = h.div_euclid(24);
            if day < today && day >= today - 7 {
                grouped.entry(day).or_default().push(s);
            }
        }
        grouped
            .into_iter()
            .filter(|(_, ss)| ss.len() >= 2)
            .map(|(d, ss)| {
                (
                    d,
                    (
                        mid(ss.iter().map(|s| s.price).collect()).unwrap(),
                        mid(ss.iter().map(|s| s.quantity).collect()).unwrap(),
                    ),
                )
            })
            .collect()
    };
    if daily.len() >= 3 {
        let reference = mid(daily.values().map(|(p, _)| *p).collect()).unwrap();
        let supply = mid(daily.values().map(|(_, q)| *q).collect()).unwrap();
        let latest = samples.iter().filter(|s| s.at <= now).max_by_key(|s| s.at);
        dump.pairs = daily.len();
        if let Some(s) = latest {
            let unusual = change(reference, s.price) <= -0.20
                && u128::from(s.quantity) >= u128::from(supply) * 2
                && supply > 0;
            dump.detected = unusual;
            dump.status = if unusual { "preliminary" } else { "watch" };
            dump.action = if !fresh {
                "Refresh scan"
            } else if unusual {
                "Monitor recovery"
            } else {
                "Wait"
            };
            dump.historical_gross_change = Some(change(reference, s.price) * 100.0);
            dump.reason = if unusual {
                "Median asking price fell at least 20% and supply doubled against three or more prior daily references. Recovery is unproven; do not treat this as a buy signal."
            } else {
                "No qualifying price-and-supply dump in the latest complete observation."
            };
        }
    }
    out.push(dump);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(at: i64, p: u64) -> Sample {
        Sample {
            at,
            price: UnitPrice::new(p, 1).unwrap(),
            quantity: 100,
        }
    }
    #[test]
    fn future_zero_and_duplicate_hours_do_not_create_coverage() {
        let now = 1_800_000_000;
        let mut ss = vec![sample(now - 1, 100); 100];
        ss.push(sample(now + 86400, 200));
        ss.push(sample(now - 86400, 0));
        let cells = heatmap(&ss, now);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].observations, 1);
        assert!(analyze(&ss, now, None, false)
            .iter()
            .all(|p| p.status == "insufficient"));
    }
    #[test]
    fn recurring_intraday_pattern_is_checked_on_later_days_and_stale_entry_is_blocked() {
        let now = DateTime::parse_from_rfc3339("2026-10-27T08:00:00+01:00")
            .unwrap()
            .timestamp();
        let mut ss = vec![];
        for d in 1..=60 {
            for h in 0..24 {
                ss.push(sample(
                    now - d * 86400 + (h - 9) * 3600,
                    if h < 12 { 100 } else { 150 },
                ));
            }
        }
        let results = analyze(&ss, now, UnitPrice::new(100, 1), false);
        let p = &results[1];
        assert!(p.training_pairs >= 2 && p.validation_pairs >= 2);
        assert_eq!(p.action, "Refresh scan");
        assert_ne!(p.status, "actionable");
    }
    #[test]
    fn daily_history_has_no_intraday_pairs_or_invented_weekly_gain() {
        let now = 1_800_000_000;
        let ss: Vec<_> = (0..40)
            .map(|d| sample(now - d * 86400, 100 + d as u64))
            .collect();
        let results = analyze(&ss, now, UnitPrice::new(100, 1), true);
        assert_eq!(results[1].status, "insufficient");
        assert!(!results[0].detected);
    }
    fn irregular_history(failed_later: bool) -> (i64, Vec<Sample>) {
        let now = DateTime::parse_from_rfc3339("2026-03-01T08:00:00+01:00")
            .unwrap()
            .timestamp();
        let mut ss = vec![];
        for day in 1..=84 {
            // Missing whole days, missing blocks, and sub-hour timestamp jitter.
            if day % 5 == 0 || (30..=33).contains(&day) {
                continue;
            }
            let entry = now - day * 86400 + (day % 3 - 1) * 900;
            let elapsed = 4 * 3600 + (day % 3 - 1) * 1200;
            ss.push(sample(entry, 100));
            ss.push(sample(
                entry + elapsed,
                if failed_later && day <= 28 { 70 } else { 150 },
            ));
        }
        (now, ss)
    }
    #[test]
    fn irregular_scans_survive_missing_dates_and_duplicates_without_pair_inflation() {
        let (now, ss) = irregular_history(false);
        let results = analyze(&ss, now, UnitPrice::new(100, 1), true);
        assert_eq!(results[1].status, "watch");
        assert_eq!(results[0].status, "watch");
        assert_eq!(results[1].holding_hours, Some(4));
        let repeated: Vec<_> = ss.iter().flat_map(|s| vec![s.clone(); 20]).collect();
        assert_eq!(
            serde_json::to_value(&results).unwrap(),
            serde_json::to_value(analyze(&repeated, now, UnitPrice::new(100, 1), true)).unwrap()
        );
        let thinned: Vec<_> = ss
            .into_iter()
            .enumerate()
            .filter(|(i, _)| (i / 2) % 2 == 0)
            .map(|(_, s)| s)
            .collect();
        assert_eq!(
            analyze(&thinned, now, UnitPrice::new(100, 1), true)[1].status,
            "watch"
        );
    }
    #[test]
    fn irregular_later_failure_cannot_change_training_target_or_holding_time() {
        let (now, good) = irregular_history(false);
        let (_, bad) = irregular_history(true);
        let pass = analyze(&good, now, None, false);
        let fail = analyze(&bad, now, None, false);
        assert_eq!(fail[1].status, "preliminary");
        assert_eq!(fail[1].validation_positive, 0);
        assert_eq!(pass[1].target, fail[1].target);
        assert_eq!(pass[1].holding_hours, fail[1].holding_hours);
        assert_eq!(pass[1].holding_range_hours, fail[1].holding_range_hours);
        assert_eq!(pass[1].historical_pairs.len(), pass[1].pairs);
        assert!(fail[1]
            .historical_pairs
            .iter()
            .filter(|p| p.later_check)
            .all(|p| p.exit_price < p.entry_price));
        assert!(pass[1]
            .historical_pairs
            .iter()
            .all(|p| p.exit_at > p.entry_at && p.exit_at < now));
    }
    #[test]
    fn large_gaps_do_not_become_intraday_exits() {
        let (now, ss) = irregular_history(false);
        let entries: Vec<_> = ss
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, s)| s)
            .collect();
        assert_eq!(analyze(&entries, now, None, false)[1].pairs, 0);
    }
    #[test]
    fn random_scan_loss_does_not_create_a_gain_in_a_stable_market() {
        let (now, source) = irregular_history(false);
        for keep_percent in [50, 20] {
            let mut state = 42_u64;
            let ss: Vec<_> = source
                .iter()
                .filter_map(|s| {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    (state % 100 < keep_percent).then(|| sample(s.at, 100))
                })
                .collect();
            let patterns = analyze(&ss, now, None, false);
            assert!(patterns.iter().all(|p| !p.detected));
        }
    }
    #[test]
    fn evening_only_collection_does_not_establish_a_morning_pattern() {
        let (now, source) = irregular_history(false);
        let evening: Vec<_> = source
            .into_iter()
            .map(|mut s| {
                s.at += 12 * 3600;
                s
            })
            .collect();
        let results = analyze(&evening, now, None, false);
        assert_eq!(results[0].pairs, 0);
        assert_eq!(results[1].pairs, 0);
    }
    #[test]
    fn elapsed_holding_time_is_measured_across_dst() {
        let now = DateTime::parse_from_rfc3339("2026-11-10T00:00:00+01:00")
            .unwrap()
            .timestamp();
        let mut ss = vec![];
        for day in 1..=80 {
            let date = local(now - day * 86400).unwrap().date_naive();
            let entry = date
                .and_hms_opt(0, 15, 0)
                .unwrap()
                .and_local_timezone(Paris)
                .single()
                .unwrap()
                .timestamp();
            ss.push(sample(entry, 100));
            ss.push(sample(entry + 4 * 3600 + 1200, 150));
        }
        assert_eq!(analyze(&ss, now, None, false)[1].holding_hours, Some(4));
    }
    fn cycle_history(validation_exit: u64) -> (i64, Vec<Sample>) {
        let now = DateTime::parse_from_rfc3339("2026-03-01T08:00:00+01:00")
            .unwrap()
            .timestamp();
        let mut ss = vec![];
        for day in 1..=63 {
            for h in 0..24 {
                let exit = if day <= 21 { validation_exit } else { 150 };
                ss.push(sample(
                    now - day * 86400 + (h - 8) * 3600,
                    if h >= 12 { exit } else { 100 },
                ));
            }
        }
        (now, ss)
    }
    #[test]
    fn later_failures_reject_a_training_pattern_without_changing_its_target() {
        let (now, good) = cycle_history(150);
        let (_, bad) = cycle_history(50);
        let pass = analyze(&good, now, UnitPrice::new(100, 1), true);
        let fail = analyze(&bad, now, UnitPrice::new(100, 1), true);
        assert_eq!(pass[1].status, "watch");
        assert_eq!(pass[1].action, "Review entry");
        assert_eq!(fail[1].status, "preliminary");
        assert_eq!(fail[1].validation_positive, 0);
        assert_eq!(pass[1].target, fail[1].target);
        assert_eq!(pass[1].holding_hours, fail[1].holding_hours);
        assert_eq!(pass[0].status, "watch");
        assert!(pass[0].pairs >= 4);
    }
    #[test]
    fn dump_requires_both_price_drop_and_supply_increase_and_never_suggests_buying() {
        let (now, mut ss) = cycle_history(150);
        ss.push(Sample {
            at: now,
            price: UnitPrice::new(50, 1).unwrap(),
            quantity: 300,
        });
        let dump = analyze(&ss, now, UnitPrice::new(50, 1), true)
            .pop()
            .unwrap();
        assert!(dump.detected);
        assert_eq!(dump.action, "Monitor recovery");
        assert!(dump.target.is_none());
        ss.last_mut().unwrap().quantity = 100;
        assert!(!analyze(&ss, now, UnitPrice::new(50, 1), true)[2].detected);
    }
    #[test]
    fn repeated_autumn_hour_has_one_vote() {
        let a = DateTime::parse_from_rfc3339("2026-10-25T02:15:00+02:00")
            .unwrap()
            .timestamp();
        let b = DateTime::parse_from_rfc3339("2026-10-25T02:15:00+01:00")
            .unwrap()
            .timestamp();
        let map = heatmap(&[sample(a, 100), sample(b, 200)], b);
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].observations, 1);
    }
}
