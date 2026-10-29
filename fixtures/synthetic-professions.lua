-- Synthetic recipe facts. These IDs and values are test data, not a verified game catalogue.
AuctionDBForeverSaved = { houses = {} }
NauctionsProfessionsSaved = {
  schema_version = 1,
  product = "wow_classic_beta",
  build = "1.60.1.70630",
  addon_version = "0.5.0-nauctions.1",
  locale = "enUS",
  recipes = {
    [900001] = {
      profession_id = 171, profession_name = "Synthetic Alchemy", name = "Synthetic Potion",
      captured_at = 1793270000, kind = "item", output_item_id = 900101,
      min_output = 2, max_output = 2, tradable = true,
      reagents = { [900201] = 3, [900202] = 1 }, tools = { "Synthetic station" },
    },
    [900002] = {
      profession_id = 333, profession_name = "Synthetic Enchanting", name = "Synthetic Enchant",
      captured_at = 1793270000, kind = "service", reagents = { [900201] = 2 }, tools = {},
    },
  },
}
