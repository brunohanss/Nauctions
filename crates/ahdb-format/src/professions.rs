use super::*;
use market_domain::crafting::{ProfessionExport, Recipe, RecipeItemDiscovery};

fn fields(t: &BTreeMap<String, V>, allowed: &[&str]) -> Result<()> {
    if t.keys().any(|k| !allowed.contains(&k.as_str())) {
        return bad("unsupported profession field");
    }
    Ok(())
}
fn text(t: &BTreeMap<String, V>, key: &str, max: usize) -> Result<String> {
    let s = get(t, key)?.string()?;
    if s.is_empty() || s.len() > max || s.chars().any(char::is_control) {
        return bad("invalid profession text");
    }
    Ok(s.into())
}
fn positive(v: &V) -> Result<u64> {
    let n = v.uint()?;
    if n == 0 || n > 1_000_000_000 {
        return bad("invalid profession ID or quantity");
    }
    Ok(n)
}
fn optional(t: &BTreeMap<String, V>, key: &str) -> Result<Option<u64>> {
    t.get(key)
        .filter(|v| !matches!(v, V::Nil))
        .map(positive)
        .transpose()
}
pub(super) fn decode(v: &V) -> Result<ProfessionExport> {
    let t = v.table()?;
    fields(
        t,
        &[
            "schema_version",
            "product",
            "build",
            "addon_version",
            "locale",
            "recipes",
            "recipe_items",
        ],
    )?;
    if get(t, "schema_version")?.uint()? != 1 {
        return bad("unsupported profession schema");
    }
    let product = text(t, "product", 40)?;
    let build = text(t, "build", 40)?;
    let parts = build.split('.').map(number).collect::<Result<Vec<_>>>()?;
    if product != "wow_classic_beta"
        || parts.len() != 4
        || parts[0] != 1
        || !(60..70).contains(&parts[1])
        || parts[3] < 69876
    {
        return bad("unsupported profession client build");
    }
    let locale = text(t, "locale", 4)?;
    if locale.len() != 4 || !locale.bytes().all(|b| b.is_ascii_alphabetic()) {
        return bad("invalid profession locale");
    }
    let entries = get(t, "recipes")?.table()?;
    if entries.len() > 20_000 {
        return bad("recipe count limit");
    }
    let mut recipes = Vec::new();
    for (key, value) in entries {
        let spell_id = positive(&V::Num(
            key.strip_prefix('\0')
                .ok_or_else(|| Error("recipe key must be numeric".into()))?
                .into(),
        ))?;
        let r = value.table()?;
        fields(
            r,
            &[
                "profession_id",
                "profession_name",
                "name",
                "captured_at",
                "kind",
                "output_item_id",
                "min_output",
                "max_output",
                "tradable",
                "reagents",
                "tools",
                "cooldown_seconds",
            ],
        )?;
        let kind = text(r, "kind", 16)?;
        if !["item", "service"].contains(&kind.as_str()) {
            return bad("invalid recipe kind");
        }
        let output_item_id = optional(r, "output_item_id")?;
        let min_output = optional(r, "min_output")?;
        let max_output = optional(r, "max_output")?;
        if (kind == "service"
            && (output_item_id.is_some() || min_output.is_some() || max_output.is_some()))
            || min_output.is_some() != max_output.is_some()
            || min_output.zip(max_output).is_some_and(|(a, b)| a > b)
            || (min_output.is_some() && output_item_id.is_none())
        {
            return bad("invalid recipe output");
        }
        let reagents = get(r, "reagents")?.table()?;
        if reagents.len() > 64 {
            return bad("reagent count limit");
        }
        let reagents = reagents
            .iter()
            .map(|(k, v)| {
                Ok((
                    positive(&V::Num(
                        k.strip_prefix('\0')
                            .ok_or_else(|| Error("reagent key must be numeric".into()))?
                            .into(),
                    ))?,
                    positive(v)?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut tools = Vec::new();
        if let Some(v) = r.get("tools") {
            let t = v.table()?;
            if t.len() > 16 {
                return bad("tool count limit");
            }
            for v in t.values() {
                let s = v.string()?;
                if s.len() > 200 || s.chars().any(char::is_control) {
                    return bad("invalid tool");
                }
                tools.push(s.to_string());
            }
        }
        let tradable = match r.get("tradable") {
            None | Some(V::Nil) => None,
            Some(V::Bool(b)) => Some(*b),
            _ => return bad("invalid tradable flag"),
        };
        recipes.push(Recipe {
            spell_id,
            profession_id: positive(get(r, "profession_id")?)?,
            profession_name: text(r, "profession_name", 200)?,
            name: text(r, "name", 300)?,
            captured_at: i64::try_from(get(r, "captured_at")?.uint()?)
                .map_err(|_| Error("invalid recipe time".into()))?,
            kind,
            output_item_id,
            min_output,
            max_output,
            tradable,
            reagents,
            tools,
            cooldown_seconds: r.get("cooldown_seconds").map(V::uint).transpose()?,
        });
    }
    let mut recipe_items = Vec::new();
    let recipe_ids: std::collections::BTreeSet<_> = recipes.iter().map(|r| r.spell_id).collect();
    if let Some(items) = t.get("recipe_items") {
        let items = items.table()?;
        if items.len() > 20_000 {
            return bad("recipe item count limit");
        }
        for (key, item) in items {
            let item_id = positive(&V::Num(
                key.strip_prefix('\0')
                    .ok_or_else(|| Error("recipe item key must be numeric".into()))?
                    .into(),
            ))?;
            let item = item.table()?;
            fields(item, &["captured_at", "recipe_spell_id", "status"])?;
            let status = text(item, "status", 32)?;
            if ![
                "resolved",
                "mapping_unavailable",
                "details_unavailable",
                "ambiguous_spell",
            ]
            .contains(&status.as_str())
            {
                return bad("invalid recipe item status");
            }
            let recipe_spell_id = optional(item, "recipe_spell_id")?;
            if status == "resolved" && recipe_spell_id.is_none_or(|id| !recipe_ids.contains(&id)) {
                return bad("resolved recipe item has no recipe facts");
            }
            if status == "details_unavailable" && recipe_spell_id.is_none() {
                return bad("recipe item details require a candidate spell");
            }
            recipe_items.push(RecipeItemDiscovery {
                item_id,
                captured_at: i64::try_from(get(item, "captured_at")?.uint()?)
                    .map_err(|_| Error("invalid recipe item time".into()))?,
                recipe_spell_id,
                status,
            });
        }
    }
    Ok(ProfessionExport {
        schema_version: 1,
        product,
        build_number: parts[3],
        build,
        addon_version: text(t, "addon_version", 40)?,
        locale,
        recipes,
        recipe_items,
    })
}
