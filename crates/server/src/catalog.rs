//! Curated display metadata. Auction observations remain the source for names,
//! categories, quantities, and prices. No network request is made on lookup.
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Deserialize)]
struct Catalog {
    game: String,
    items: Vec<Entry>,
}

#[derive(Deserialize)]
pub struct Entry {
    id: i64,
    pub icon: String,
    source_url: String,
}

#[derive(Clone, Serialize)]
pub struct Attribution {
    text: &'static str,
    url: String,
}

impl Entry {
    pub fn attribution(&self) -> Attribution {
        Attribution {
            text: "Item metadata: Wowhead. Game artwork: Blizzard Entertainment.",
            url: self.source_url.clone(),
        }
    }
}

pub fn lookup(item_id: i64) -> Option<&'static Entry> {
    static ITEMS: OnceLock<BTreeMap<i64, Entry>> = OnceLock::new();
    ITEMS
        .get_or_init(|| {
            let catalog: Catalog =
                serde_json::from_str(include_str!("../../../assets/item-catalog.json"))
                    .expect("embedded item catalogue must be valid JSON");
            assert_eq!(catalog.game, "forever", "wrong catalogue game");
            let mut items = BTreeMap::new();
            for entry in catalog.items {
                assert!(entry.id > 0, "invalid catalogue item ID");
                assert_eq!(entry.icon, format!("/item-icons/{}.jpg", entry.id));
                assert!(entry
                    .source_url
                    .starts_with("https://www.wowhead.com/classic/item="));
                assert!(items.insert(entry.id, entry).is_none(), "duplicate item ID");
            }
            items
        })
        .get(&item_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_item_has_local_icon_and_source_attribution() {
        let entry = lookup(2589).expect("Linen Cloth is in the reviewed catalogue");
        assert_eq!(entry.icon, "/item-icons/2589.jpg");
        assert!(entry.attribution().url.ends_with("item=2589"));
    }

    #[test]
    fn unknown_item_has_no_invented_metadata() {
        assert!(lookup(i64::MAX).is_none());
        assert!(lookup(-1).is_none());
    }
}
