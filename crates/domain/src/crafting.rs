//! Shared recipe facts. No character identity or learned-recipe state.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Recipe {
    pub spell_id: u64,
    pub profession_id: u64,
    pub profession_name: String,
    pub name: String,
    pub captured_at: i64,
    pub kind: String,
    pub output_item_id: Option<u64>,
    pub min_output: Option<u64>,
    pub max_output: Option<u64>,
    pub tradable: Option<bool>,
    pub reagents: BTreeMap<u64, u64>,
    pub tools: Vec<String>,
    pub cooldown_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfessionExport {
    pub schema_version: u64,
    pub product: String,
    pub build: String,
    pub build_number: u64,
    pub addon_version: String,
    pub locale: String,
    pub recipes: Vec<Recipe>,
    #[serde(default)]
    pub recipe_items: Vec<RecipeItemDiscovery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeItemDiscovery {
    pub item_id: u64,
    pub captured_at: i64,
    pub recipe_spell_id: Option<u64>,
    pub status: String,
}

impl Recipe {
    /// Economic identity excludes local names, capture time, binding metadata and temporary cooldown remaining.
    /// Same-build disagreements remain separate candidates, rather than last-writer wins.
    pub fn fingerprint(&self, product: &str, build: &str) -> String {
        use sha2::{Digest, Sha256};
        let bytes = serde_json::to_vec(&(
            "nauctions-recipe-v1",
            product,
            build,
            self.spell_id,
            self.profession_id,
            &self.kind,
            self.output_item_id,
            self.min_output,
            self.max_output,
            &self.reagents,
        ))
        .expect("serializable recipe");
        format!("{:x}", Sha256::digest(bytes))
    }
}
