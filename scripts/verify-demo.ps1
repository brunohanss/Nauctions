param([string]$Origin = 'http://127.0.0.1:8080')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$manifest = Get-Content (Join-Path $root 'artifacts/demo/scenarios.json') -Raw | ConvertFrom-Json
$password = [IO.File]::ReadAllText((Join-Path $root 'data/dev-admin-password.txt')).Trim()
$body = @{login='admin'; password=$password} | ConvertTo-Json
$null = Invoke-RestMethod "$Origin/v1/auth/login" -Method Post -Headers @{Origin=$Origin} -ContentType 'application/json' -Body $body -SessionVariable session
$markets = @(Invoke-RestMethod "$Origin/v1/markets" -WebSession $session)
if ($markets.Count -ne 1 -or !$markets[0].demo -or !$markets[0].read_only) { throw 'Expected one read-only demo market.' }
if ([DateTimeOffset]$markets[0].as_of -ne [DateTimeOffset]$manifest.anchor) { throw 'Replay time differs from fixture anchor.' }
$items = Invoke-RestMethod "$Origin/v1/items?limit=100" -WebSession $session
if ($items.items.Count -ne 50) { throw 'Expected 50 items.' }
$offers = Invoke-RestMethod "$Origin/v1/opportunities?limit=100&sort=score&direction=desc" -WebSession $session
$expectedKeys = @($manifest.scenarios | Where-Object scenario -eq 'real_bargain' | ForEach-Object item_key | Sort-Object)
$actualKeys = @($offers.items | ForEach-Object item_key | Sort-Object)
if (Compare-Object $expectedKeys $actualKeys) { throw 'Opportunity membership differs from expected bargains.' }
foreach ($category in @('materials','consumables','recipes','equipment','other')) {
    $categoryItems = Invoke-RestMethod "$Origin/v1/items?limit=100&category=$category" -WebSession $session
    $categoryOffers = Invoke-RestMethod "$Origin/v1/opportunities?limit=100&category=$category" -WebSession $session
    if ($categoryItems.items.Count -ne 10) { throw "Expected 10 items in $category." }
    if ($categoryOffers.items.Count -ne 2) { throw "Expected two opportunities in $category." }
    if (@($categoryItems.items | Where-Object category -ne $category).Count) { throw "Incorrect category filter: $category." }
}
$lastScore = [double]::PositiveInfinity
foreach ($offer in $offers.items) {
    if ($offer.opportunity_score -gt $lastScore -or $offer.discount_percentage -gt -30) { throw 'Opportunity order or discount is invalid.' }
    $lastScore = $offer.opportunity_score
}
foreach ($item in $items.items) {
    if ($item.source_kind -ne 'demo' -or $item.freshness_seconds -ne 0 -or $item.baseline.day_count -ne 7) { throw "Invalid replay observation: $($item.item_key)" }
    $icon = Invoke-WebRequest "$Origin$($item.icon)" -UseBasicParsing
    if ($icon.StatusCode -ne 200 -or $icon.Headers['Content-Type'] -notlike 'image/jpeg*') { throw "Missing local icon: $($item.item_key)" }
    $depth = Invoke-RestMethod "$Origin/v1/items/$($item.item_key)/depth" -WebSession $session
    if (!$depth.available -or ($depth.listings | Measure-Object stack_count -Sum).Sum -ne $item.total_units) { throw "Depth quantity mismatch: $($item.item_key)" }
}
$history = Invoke-RestMethod "$Origin/v1/items/$($items.items[0].item_key)/history?range=all" -WebSession $session
if ($history.points.Count -ne $manifest.scan_count) { throw 'History does not include every fixture snapshot.' }
$strategies = Invoke-RestMethod "$Origin/v1/strategies" -WebSession $session
if ($strategies.method -ne 'exploratory_v2_irregular_pairs') { throw 'Expected irregular strategy analysis. Rebuild and restart the preview.' }
$expected = Get-Content (Join-Path $root 'artifacts/demo/results-irregular.json') -Raw | ConvertFrom-Json
foreach ($row in $expected) {
    $actual = $strategies.items | Where-Object { $_.item.item_key -eq $row.item_key }
    if (!$actual) { throw "Missing strategy item: $($row.item_key)" }
    foreach ($pattern in $row.patterns) {
        $match = $actual.patterns | Where-Object strategy -eq $pattern.strategy
        if ($match.detected -ne $pattern.detected -or $match.status -ne $pattern.status -or $match.pairs -ne $pattern.pairs -or $match.holding_hours -ne $pattern.holding_hours) {
            throw "API strategy differs from analysis fixture: $($row.item_key) / $($pattern.strategy)"
        }
    }
}
$watch = @($strategies.items.patterns | Where-Object { $_.detected -and $_.status -eq 'watch' }).Count
Write-Host "Verified strategy API against the Rust fixture results: $watch detected Watch patterns."
Write-Host "Verified 50 items (10 per category), 50 local icons, 50 listing depths, $($actualKeys.Count) expected opportunities (two per category), fixed replay time and $($history.points.Count) history points."
