# Synthetic strategy datasets

Use this format to test strategy analysis with irregular observations. This is
an analysis fixture, not an AHDB SavedVariables file or a database seed.

## Generation instructions

```text
Generate dataset.json as UTF-8 JSON. Mark all data as synthetic asking prices.
Use schema_version=1, synthetic=true, seed=42, and a fixed as_of Unix timestamp.
Include market.region, market.realm, market.house, and an items array.
Each item must have item_key, scenario, and samples.
Each sample must have at, price.numerator, price.denominator, and quantity.
Use UTC Unix seconds for timestamps. Use positive integer strings for prices.
Prices are rational copper per unit. Quantity is total available units.
Each sample represents an item in a complete scan of this single market.

Generate 84 days of latent hourly prices and supply with a fixed random seed.
Select irregular scan times from that market. Sort samples by timestamp.
Keep timestamps at or before as_of. Include a recent scan within 45 minutes.
Do not fill missing scans, invent sales, or insert zero prices for missing data.
Vary supply and prices independently of scan frequency.

Include these scenarios:
- stable: small fluctuations without a recurring gain;
- intraday_recovery: repeated gains above 20% after 4–12 hours;
- weekly_cycle: repeated weekday gains above 20% after 1–4 days;
- failed_validation: early gains disappear in the final third;
- sparse: too few comparable entries and exits;
- market_dump: latest price falls at least 20% and supply at least doubles.
Retain positive examples in both early and late history for recurring scenarios.
Do not assume that a dump will recover.

Create separate files from the same latent market:
- dense reference;
- random removal of 50% of scans;
- random removal of 80% of scans;
- evening-only collection;
- missing blocks of 1–3 days;
- repeated identical samples during scan bursts.
Use the same removal schedule for items in the same complete market scan.
Change scan availability, not the underlying prices, between variants.
Include README.md with seed, generation rules, and intended scenarios.
Record intended outcomes separately from measured model results.
Sparse variants are allowed to produce insufficient evidence.
```

Example structure (two observations demonstrate the schema, not enough evidence):

```json
{
  "schema_version": 1,
  "synthetic": true,
  "seed": 42,
  "as_of": 1793170800,
  "market": {"region": "EU", "realm": "Synthetic Test", "house": "Alliance"},
  "items": [{
    "item_key": "2589",
    "scenario": "intraday_recovery",
    "samples": [
      {"at": 1793084400, "price": {"numerator": "10000", "denominator": "1"}, "quantity": 200},
      {"at": 1793099700, "price": {"numerator": "13000", "denominator": "1"}, "quantity": 180}
    ]
  }]
}
```

## Inspect a dataset

Run from the repository root in PowerShell:

```powershell
cargo run -p market-analysis --bin strategy-inspect -- artifacts/strategy/dataset.json
```

The command prints model results per item. It does not write to the database.
It uses the latest observation to check the current price and freshness. It
rejects future observations, zero prices, and unsupported fixture versions.
It reports the supplied scenario as a label; it does not use that label to
select a strategy. Market and seed metadata are descriptive only.

Check that duplicates do not increase pair counts, missing exits do not create
pairs, and failed later validation prevents Watch status. Compare each sparse
variant with the dense reference. Fewer observations can remove a signal;
they must not be described as evidence that the price stayed unchanged.
