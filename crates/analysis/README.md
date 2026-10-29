# Market analysis

`snapshot` calculates one snapshot at a time. It never adds inventories from different scans. The buyout median is weighted by stack quantity. It uses the lower weighted median when the cumulative weight reaches half of the buyout units. Bid-only units count toward total units, but they do not affect buyout prices.

`baseline` uses seven completed Europe/Paris calendar days. It takes the lower median within each day, then the lower median of those daily medians. At least three days must have eligible prices. Each date has one vote. `source_count` reports eligible source coverage; it is not an extra weight. Callers must remove duplicate accepted scan identities before they provide samples. Inherited AHDB daily data must not overlap raw scan inputs.

`evaluate` rejects unknown eligibility, zero prices, future observations, and observations older than 45 minutes. The caller must verify market, completeness, item identity, and source quality before it sets `eligible`. Underpriced signals need at least three baseline days. Threshold signals use an explicit positive limit. Movement signals compare with a positive previous price. All comparisons use exact rational prices. Signal generation does not send notifications. The server must save deduplication and delivery state in a transaction.
