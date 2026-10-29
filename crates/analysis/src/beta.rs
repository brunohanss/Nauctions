//! A dated reference window is not evidence of beta reset or launch age.
use crate::forecast::{self, Estimate};
use market_domain::UnitPrice;
use serde::{Deserialize, Serialize};
pub type ReferencePoint = (u32, Option<u64>, Option<u64>, Option<u64>);

#[derive(Clone, Serialize, Deserialize)]
pub struct ReferenceItem {
    pub id: u64,
    pub name: String,
    pub variants_combined: bool,
    pub points: Vec<ReferencePoint>,
}
#[derive(Deserialize)]
pub struct Reference {
    pub schema_version: u32,
    pub source_realm: String,
    pub house: String,
    pub reference_date: String,
    pub source_updated_at: String,
    pub source_sha256: String,
    pub daily_rows: usize,
    pub attribution: serde_json::Value,
    pub items: Vec<ReferenceItem>,
}
/// Log-linear interpolation is a scenario assumption, never an observed price.
fn level(item: &ReferenceItem, day: f64) -> Option<f64> {
    let points: Vec<_> = item
        .points
        .iter()
        .filter_map(|p| {
            p.2.filter(|n| *n > 0)
                .map(|n| (f64::from(p.0), (n as f64).ln()))
        })
        .collect();
    if let Some((_, value)) = points.iter().find(|p| (p.0 - day).abs() < 1e-8) {
        return Some(*value);
    }
    points
        .windows(2)
        .find(|w| w[0].0 < day && day < w[1].0)
        .map(|w| w[0].1 + (w[1].1 - w[0].1) * (day - w[0].0) / (w[1].0 - w[0].0))
}
pub fn scenario(
    item: Option<&ReferenceItem>,
    day: f64,
    anchor: Option<UnitPrice>,
    now: i64,
) -> Vec<Estimate> {
    [24,72,168].into_iter().map(|hours| {
        let mut out=forecast::empty(now,hours,"No comparable beta medians cover both scenario dates.");
        let Some(item)=item else {return out;};
        out.observations=item.points.iter().filter(|p|p.2.is_some_and(|n|n>0)).count();
        if !day.is_finite() || !(0.0..=17.0).contains(&day) {return out;}
        let (Some(start),Some(end))=(level(item,day),level(item,day+hours as f64/24.0)) else {return out;};
        let ratio=(end-start).exp();
        out.status="scenario"; out.method="beta_reference_log_interpolation";
        out.reason="Unvalidated scenario from four beta dates. Reference day zero is October 3, not a verified beta reset. No extrapolation.";
        out.range_kind="assumed_log_margin_0_25_not_probability";
        out.change_percent=Some((ratio-1.0)*100.0);
        out.direction=if ratio>1.05 {"up"} else if ratio<0.95 {"down"} else {"stable"};
        if let Some(anchor)=anchor.filter(|p|p.numerator()>0 && !item.variants_combined) {
            let anchor=anchor.numerator() as f64/anchor.denominator() as f64;
            forecast::fill(&mut out,anchor,anchor*ratio,anchor*ratio*(-0.25_f64).exp(),anchor*ratio*0.25_f64.exp());
            if out.price.is_none() || out.high.is_none() {return forecast::empty(now,hours,"Scenario exceeds the supported price range.");}
        }
        out
    }).collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interpolation_is_explicit_and_does_not_extrapolate() {
        let item = ReferenceItem {
            id: 1,
            name: "Item".into(),
            variants_combined: false,
            points: vec![
                (0, Some(100), Some(100), Some(1)),
                (8, Some(200), Some(200), Some(1)),
            ],
        };
        let f = scenario(Some(&item), 0.0, UnitPrice::new(1000, 1), 100);
        assert_eq!(f[0].status, "scenario");
        assert!(f[0].price.is_some());
        assert!(scenario(Some(&item), 8.0, UnitPrice::new(1000, 1), 100)
            .iter()
            .all(|f| f.price.is_none()));
        let f = scenario(Some(&item), 0.0, None, 100);
        assert!(f[0].price.is_none());
        assert!(f[0].change_percent.is_some());
    }

    #[test]
    fn combined_variants_keep_only_relative_scenarios() {
        let item = ReferenceItem {
            id: 1,
            name: "Combined item".into(),
            variants_combined: true,
            points: vec![(0, None, Some(100), None), (8, None, Some(200), None)],
        };
        let estimates = scenario(Some(&item), 0.0, UnitPrice::new(1000, 1), 100);
        for estimate in estimates {
            assert_eq!(estimate.status, "scenario");
            assert!(estimate.change_percent.is_some());
            assert!(estimate.price.is_none());
            assert!(estimate.low.is_none());
            assert!(estimate.high.is_none());
        }
    }
}
