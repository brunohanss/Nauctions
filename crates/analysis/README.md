# Market analysis

`snapshot` calculates one snapshot at a time. It never adds inventories from different scans. The buyout median is weighted by stack quantity. It uses the lower weighted median when the cumulative weight reaches half of the buyout units. Bid-only units count toward total units, but they do not affect buyout prices.

`baseline` uses seven completed Europe/Paris calendar days. It takes the lower median within each day, then the lower median of those daily medians. At least three days must have eligible prices. Each date has one vote. `source_count` reports eligible source coverage; it is not an extra weight. Callers must remove duplicate accepted scan identities before they provide samples. Inherited AHDB daily data must not overlap raw scan inputs.

`evaluate` rejects unknown eligibility, zero prices, future observations, and observations older than 45 minutes. The caller must verify market, completeness, item identity, and source quality before it sets `eligible`. Underpriced signals need at least three baseline days. Threshold signals use an explicit positive limit. Movement signals compare with a positive previous price. All comparisons use exact rational prices. Signal generation does not send notifications. The server must save deduplication and delivery state in a transaction.

## Forecasts

`forecast::rolling` estimates the item median asking price about 24 hours ahead.
The caller supplies only complete, canonical full scans for one exact market and
item identity. Daily imports and the bundled beta reference are excluded.

Keep one real observation per UTC hour. Require a fresh eligible anchor, at least
48 observed hours across seven days, and 36 comparable historical 24-hour pairs.
An actual target observation must be within one hour of its target time. Origins
are at least six hours apart. Missing observations are not filled.

Compare three methods: unchanged price, damped recent trend, and same-hour median.
Use chronological thirds for selection, promotion against the baseline, and later
evaluation. Skip four origins between periods to keep their 24-hour target windows
apart. Promotion requires at least a 5% error reduction. If the selected method
fails the untouched later baseline comparison, publish no estimate. Otherwise,
report its later mean absolute log error and baseline error. Forecast prices are
approximate rational copper values with up to four decimal places.

The range uses the 90th percentile of absolute later log errors, with a minimum
log margin of 0.05. It is not a calibrated probability interval. Overlapping
targets within a period remain correlated; the comparison count is not an
independent-trial count. There are no sale or liquidity claims.

`beta::scenario` uses dated Normal Horde reference points. October 3, 2026 is the
reference day, not a verified beta reset. Log-linear interpolation is a scenario
assumption between observed dates. Do not extrapolate. A scenario can show a
relative change without a live anchor. Do not produce an absolute item price when
the source combines random variants. The scenario's assumed log margin is 0.25;
it has no probability attached. `forecast::launch` separately supports imported
runs with known reset times and a configured live launch time.
