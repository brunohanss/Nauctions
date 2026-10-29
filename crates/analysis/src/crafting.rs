//! Exact signed crafting estimates from one auction snapshot.
use market_domain::{crafting::Recipe, Listing, UnitPrice};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Amount {
    pub numerator: i128,
    pub denominator: i128,
}
impl Serialize for Amount {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("Amount", 2)?;
        s.serialize_field("numerator", &self.numerator.to_string())?;
        s.serialize_field("denominator", &self.denominator.to_string())?;
        s.end()
    }
}
fn gcd(mut a: i128, mut b: i128) -> i128 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a.max(1)
}
impl Amount {
    pub fn integer(n: u64) -> Self {
        Self {
            numerator: n.into(),
            denominator: 1,
        }
    }
    fn new(n: i128, d: i128) -> Option<Self> {
        if d <= 0 {
            return None;
        }
        let g = gcd(n.checked_abs()?, d);
        Some(Self {
            numerator: n / g,
            denominator: d / g,
        })
    }
    fn price(p: UnitPrice) -> Self {
        Self::new(p.numerator().into(), p.denominator().into()).unwrap()
    }
    fn add(self, other: Self) -> Option<Self> {
        let g = gcd(self.denominator, other.denominator);
        let a = other.denominator / g;
        let b = self.denominator / g;
        Self::new(
            self.numerator
                .checked_mul(a)?
                .checked_add(other.numerator.checked_mul(b)?)?,
            self.denominator.checked_mul(a)?,
        )
    }
    fn scale(self, n: i128, d: i128) -> Option<Self> {
        let g = gcd(n.checked_abs()?, self.denominator);
        let h = gcd(self.numerator.checked_abs()?, d);
        Self::new(
            (self.numerator / h).checked_mul(n / g)?,
            (self.denominator / g).checked_mul(d / h)?,
        )
    }
    fn subtract(self, other: Self) -> Option<Self> {
        self.add(Self {
            numerator: other.numerator.checked_neg()?,
            ..other
        })
    }
    fn percent(self, cost: Self) -> Option<Self> {
        if cost.numerator <= 0 {
            return None;
        }
        self.scale(cost.denominator.checked_mul(100)?, cost.numerator)
    }
}

#[derive(Debug, Serialize)]
pub struct ReagentCost {
    pub item_id: u64,
    pub required_units: u64,
    pub purchased_units: Option<u64>,
    pub unit_price: Option<UnitPrice>,
    pub cost: Option<Amount>,
    pub status: &'static str,
}
#[derive(Debug, Serialize)]
pub struct Quote {
    pub status: &'static str,
    pub mode: String,
    pub crafts: u64,
    pub reagents: Vec<ReagentCost>,
    pub material_cost: Option<Amount>,
    pub total_cost: Option<Amount>,
    pub cost_per_output: Option<Amount>,
    pub output_units: Option<u64>,
    pub sale_unit_price: Option<UnitPrice>,
    pub gross_sale_value: Option<Amount>,
    pub net_sale_value: Option<Amount>,
    pub estimated_profit: Option<Amount>,
    pub margin_percent: Option<Amount>,
    pub return_on_cost_percent: Option<Amount>,
    pub fee_bps: u64,
    pub additional_cost_copper: String,
}

pub fn calculate(
    recipe: &Recipe,
    listings: &[Listing],
    crafts: u64,
    mode: &str,
    fee_bps: u64,
    additional_cost: u64,
) -> Option<Quote> {
    if crafts == 0
        || crafts > 10_000
        || fee_bps > 10_000
        || !["reference", "purchase"].contains(&mode)
    {
        return None;
    }
    let mut by_item: BTreeMap<String, Vec<&Listing>> = BTreeMap::new();
    for l in listings.iter().filter(|l| l.unit_buyout().is_some()) {
        by_item.entry(l.item_key.clone()).or_default().push(l);
    }
    for rows in by_item.values_mut() {
        rows.sort_by_key(|l| l.unit_buyout());
    }
    let mut reagents = Vec::new();
    let mut total = Some(Amount::integer(0));
    for (&id, &quantity) in &recipe.reagents {
        let required = quantity.checked_mul(crafts)?;
        let rows = by_item.get(&format!("i{id}"));
        let unit_price = rows.and_then(|r| r.first()).and_then(|l| l.unit_buyout());
        let (cost, purchased, status) = if mode == "reference" {
            (
                unit_price.and_then(|p| Amount::price(p).scale(required.into(), 1)),
                None,
                if unit_price.is_some() {
                    "priced"
                } else {
                    "missing_price"
                },
            )
        } else {
            let (mut units, mut cost) = (0u64, 0u64);
            for row in rows.into_iter().flatten() {
                if units >= required {
                    break;
                }
                units = units.checked_add(row.stack_count)?;
                cost = cost.checked_add(row.buyout_total_copper?)?;
            }
            if units < required {
                (None, Some(units), "insufficient_supply")
            } else {
                (Some(Amount::integer(cost)), Some(units), "priced")
            }
        };
        total = match (total, cost) {
            (Some(a), Some(b)) => Some(a.add(b)?),
            _ => None,
        };
        reagents.push(ReagentCost {
            item_id: id,
            required_units: required,
            purchased_units: purchased,
            unit_price,
            cost,
            status,
        });
    }
    let material_cost = total;
    let total_cost = total
        .map(|c| c.add(Amount::integer(additional_cost)))
        .transpose_option()?;
    let output_units = match (recipe.min_output, recipe.max_output) {
        (Some(a), Some(b)) if a == b => Some(a.checked_mul(crafts)?),
        _ => None,
    };
    let sale_unit_price = recipe
        .output_item_id
        .and_then(|id| by_item.get(&format!("i{id}")))
        .and_then(|r| r.first())
        .and_then(|l| l.unit_buyout());
    let eligible = recipe.kind == "item" && recipe.tradable != Some(false);
    let gross = if eligible {
        match (output_units, sale_unit_price) {
            (Some(n), Some(p)) => Some(Amount::price(p).scale(n.into(), 1)?),
            _ => None,
        }
    } else {
        None
    };
    let net = gross
        .map(|g| g.scale((10_000 - fee_bps).into(), 10_000))
        .transpose_option()?;
    let profit = match (net, total_cost) {
        (Some(a), Some(b)) => Some(a.subtract(b)?),
        _ => None,
    };
    let status = if total_cost.is_none() {
        "missing_materials"
    } else if !eligible {
        "not_auctionable"
    } else if output_units.is_none() {
        "unknown_or_variable_yield"
    } else if sale_unit_price.is_none() {
        "missing_sale_price"
    } else {
        "estimated"
    };
    Some(Quote {
        status,
        mode: mode.into(),
        crafts,
        reagents,
        material_cost,
        total_cost,
        cost_per_output: match (total_cost, output_units) {
            (Some(c), Some(n)) => Some(c.scale(1, n.into())?),
            _ => None,
        },
        output_units,
        sale_unit_price,
        gross_sale_value: gross,
        net_sale_value: net,
        estimated_profit: profit,
        margin_percent: match (profit, net) {
            (Some(p), Some(n)) => p.percent(n),
            _ => None,
        },
        return_on_cost_percent: match (profit, total_cost) {
            (Some(p), Some(c)) => p.percent(c),
            _ => None,
        },
        fee_bps,
        additional_cost_copper: additional_cost.to_string(),
    })
}
trait TransposeOption<T> {
    fn transpose_option(self) -> Option<Option<T>>;
}
impl<T> TransposeOption<T> for Option<Option<T>> {
    fn transpose_option(self) -> Option<Option<T>> {
        match self {
            None => Some(None),
            Some(v) => v.map(Some),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn recipe() -> Recipe {
        Recipe {
            spell_id: 1,
            profession_id: 171,
            profession_name: "Alchemy".into(),
            name: "Test".into(),
            captured_at: 1,
            kind: "item".into(),
            output_item_id: Some(10),
            min_output: Some(2),
            max_output: Some(2),
            tradable: Some(true),
            reagents: BTreeMap::from([(20, 3)]),
            tools: vec![],
            cooldown_seconds: None,
        }
    }
    fn listing(key: &str, units: u64, cost: Option<u64>) -> Listing {
        Listing {
            item_key: key.into(),
            stack_count: units,
            buyout_total_copper: cost,
            min_bid_copper: 0,
            current_bid_copper: 0,
            time_left_band: 1,
        }
    }
    #[test]
    fn exact_batch_yield_fees_and_negative_profit() {
        let r = recipe();
        let rows = vec![listing("i20", 3, Some(100)), listing("i10", 1, Some(200))];
        let q = calculate(&r, &rows, 1, "reference", 500, 0).unwrap();
        assert_eq!(q.total_cost, Some(Amount::integer(100)));
        assert_eq!(q.cost_per_output, Some(Amount::integer(50)));
        assert_eq!(q.estimated_profit, Some(Amount::integer(280)));
        let q = calculate(&r, &rows, 2, "reference", 500, 1000).unwrap();
        assert_eq!(q.estimated_profit, Some(Amount::new(-440, 1).unwrap()));
        let q = calculate(
            &r,
            &[listing("i20", 7, Some(10)), listing("i10", 3, Some(11))],
            1,
            "reference",
            500,
            0,
        )
        .unwrap();
        assert_eq!(q.estimated_profit, Some(Amount::new(563, 210).unwrap()));
    }
    #[test]
    fn purchase_uses_whole_stacks_and_reports_missing_supply() {
        let rows = vec![
            listing("i20", 2, Some(10)),
            listing("i20", 2, Some(20)),
            listing("i10", 1, Some(100)),
        ];
        let q = calculate(&recipe(), &rows, 1, "purchase", 0, 0).unwrap();
        assert_eq!(q.material_cost, Some(Amount::integer(30)));
        assert_eq!(q.reagents[0].purchased_units, Some(4));
        let q = calculate(&recipe(), &rows, 2, "purchase", 0, 0).unwrap();
        assert_eq!(q.status, "missing_materials");
        assert_eq!(q.estimated_profit, None);
    }
    #[test]
    fn variants_bid_only_unknown_yields_and_services_do_not_invent_profit() {
        let rows = vec![
            listing("i20:5", 10, Some(1)),
            listing("i20", 1, None),
            listing("i10", 1, Some(100)),
        ];
        assert_eq!(
            calculate(&recipe(), &rows, 1, "reference", 0, 0)
                .unwrap()
                .status,
            "missing_materials"
        );
        let rows = vec![listing("i20", 1, Some(10)), listing("i10", 1, Some(100))];
        let mut r = recipe();
        r.max_output = Some(3);
        assert_eq!(
            calculate(&r, &rows, 1, "reference", 0, 0).unwrap().status,
            "unknown_or_variable_yield"
        );
        r.kind = "service".into();
        assert_eq!(
            calculate(&r, &rows, 1, "reference", 0, 0)
                .unwrap()
                .estimated_profit,
            None
        );
        r.kind = "item".into();
        r.tradable = Some(false);
        assert_eq!(
            calculate(&r, &rows, 1, "reference", 0, 0).unwrap().status,
            "not_auctionable"
        );
    }
}
