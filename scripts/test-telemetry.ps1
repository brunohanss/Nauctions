param([string]$Configuration = (Join-Path (Split-Path $PSScriptRoot -Parent) 'telemetry.local.json'))
$ErrorActionPreference = 'Stop'
$key = $env:POSTHOG_API_KEY
if (!$key -and (Test-Path -LiteralPath $Configuration)) { $key = (Get-Content -LiteralPath $Configuration -Raw | ConvertFrom-Json).project_key }
if (!$key -or !$key.StartsWith('phc_')) { throw 'Set POSTHOG_API_KEY to the project ingestion key.' }
$hostUrl = 'https://eu.i.posthog.com'
$id = [Guid]::NewGuid().ToString()
$properties = @{ distinct_id = 'nauctions-telemetry-check'; '$process_person_profile' = $false; '$geoip_disable' = $true; operation = 'telemetry_smoke_test'; incident_id = $id; component = 'verification' }
$event = @{ api_key = $key; event = 'nauctions.telemetry_smoke_test'; properties = $properties } | ConvertTo-Json -Depth 12
$result = Invoke-WebRequest -Uri "$hostUrl/i/v0/e/" -Method Post -ContentType 'application/json' -Body $event -UseBasicParsing -TimeoutSec 10
Write-Output "Product event accepted: HTTP $($result.StatusCode)"
$properties['$exception_list'] = @(@{ type = 'NauctionsVerification'; value = 'Monitoring verification. No user data.'; mechanism = @{ type = 'generic'; handled = $true } })
$exception = @{ api_key = $key; event = '$exception'; properties = $properties } | ConvertTo-Json -Depth 12
$result = Invoke-WebRequest -Uri "$hostUrl/i/v0/e/" -Method Post -ContentType 'application/json' -Body $exception -UseBasicParsing -TimeoutSec 10
Write-Output "Exception accepted: HTTP $($result.StatusCode)"
$unixNano = ([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() * [long]1000000).ToString()
$logs = @{ resourceLogs = @(@{ resource = @{ attributes = @(@{ key = 'service.name'; value = @{ stringValue = 'nauctions-verification' } }) }; scopeLogs = @(@{ scope = @{ name = 'nauctions' }; logRecords = @(@{ timeUnixNano = $unixNano; severityNumber = 9; severityText = 'INFO'; body = @{ stringValue = 'nauctions.telemetry_smoke_test' }; attributes = @(@{ key = 'incident_id'; value = @{ stringValue = $id } }) }) }) }) } | ConvertTo-Json -Depth 18
$result = Invoke-WebRequest -Uri "$hostUrl/i/v1/logs" -Method Post -Headers @{ Authorization = "Bearer $key" } -ContentType 'application/json' -Body $logs -UseBasicParsing -TimeoutSec 10
Write-Output "Structured log accepted: HTTP $($result.StatusCode)"
Write-Output "Find incident_id=$id in PostHog EU. HTTP acceptance does not prove dashboard processing."
