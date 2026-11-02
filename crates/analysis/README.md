# Market analysis

`snapshot` calculates one snapshot at a time. It never adds inventories from different scans. The buyout median is weighted by stack quantity. It uses the lower weighted median when the cumulative weight reaches half of the buyout units. Bid-only units count toward total units, but they do not affect buyout prices.

`baseline` uses seven completed Europe/Paris calendar days. It takes the lower median within each day, then the lower median of those daily medians. At least three days must have eligible prices. Each date has one vote. `source_count` reports eligible source coverage; it is not an extra weight. Callers must remove duplicate accepted scan identities before they provide samples. Inherited AHDB daily data must not overlap raw scan inputs.

`evaluate` rejects unknown eligibility, zero prices, future observations, and observations older than 45 minutes. The caller must verify market, completeness, item identity, and source quality before it sets `eligible`. Underpriced signals need at least three baseline days. Threshold signals use an explicit positive limit. Movement signals compare with a positive previous price. All comparisons use exact rational prices. Signal generation does not send notifications. The server must save deduplication and delivery state in a transaction.

## Forecasts

`forecast::rolling` estimates the item median asking price about 24 hours ahead.
The caller supplies only complete, canonical full scans for one exact market and
item identity. Daily imports are excluded.

Keep one real observation per UTC hour. Require a fresh eligible anchor and at
least three distinct Europe/Paris calendar dates. Each qualifying date must have
at least two observations four hours apart. Morning and evening scans, with an
optional midday scan, satisfy this rule. The dates need not be consecutive.
Missing observations are not filled.

Publish a provisional 24-hour trend when at least three real next-day changes
are available over the latest four days. Match target observations within three
hours. Origins are at least six hours apart. Apply half of the median log-price
change to the latest price. Limit the log change to plus or minus 0.35 before
damping. This fixed rule does not claim improvement over the baseline.

Measure accuracy with historical predictions that use only observations available
at each origin. Compare their errors with the unchanged-price baseline on the
same origins. With three days, the trend can be available before any later error
check exists. In that case, publish no error statistic or range. When checks exist,
use their 90th percentile absolute log error, with a minimum margin of 0.10, for
the provisional range. Label all provisional results as limited history.

If scan times do not supply three next-day changes, publish a basic unchanged-price
estimate instead. Its error evidence uses actual next-day comparisons and a
minimum log margin of 0.05. Without comparisons, it has no range or error statistic.

Trend model selection still requires 36 historical 24-hour comparisons, after a
three-day and 24-observation warmup. For these comparisons, the target observation
must be within one hour. Origins are at least six hours apart.

Compare three methods: unchanged price, damped recent trend, and same-hour median.
Use chronological thirds for selection, promotion against the baseline, and later
evaluation. Skip four origins between periods to keep their 24-hour target windows
apart. Promotion requires at least a 5% error reduction. If the selected method
fails the untouched later baseline comparison, publish no estimate. Otherwise,
report its later mean absolute log error and baseline error. Forecast prices are
approximate rational copper values with up to four decimal places.

For later-validated models, the range uses the 90th percentile of absolute later
log errors, with a minimum
log margin of 0.05. It is not a calibrated probability interval. Overlapping
targets within a period remain correlated; the comparison count is not an
independent-trial count. There are no sale or liquidity claims.
