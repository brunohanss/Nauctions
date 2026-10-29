-- Nauctions forecasts. Beta references never enter AHDB history or native scan exports.
local _, ADB = ...

local function finite(n) return type(n) == "number" and n == n and math.abs(n) < math.huge end
local function amount(price)
  if type(price) ~= "table" then return end
  local n, d = tonumber(price.numerator), tonumber(price.denominator)
  if not finite(n) or not finite(d) or n <= 0 or d <= 0 or n > 9007199254740991 or d > 9007199254740991 then return end
  return n / d
end
function ADB:ForecastItemKey(link)
  if type(link) ~= "string" then return end
  local key = link:match("^|[^|]+|H([^|]+)|")
  if not key then return end
  key = key:gsub("^(.)[^:]+:", "%1")
  key = key:gsub("^(i[-%d]+:[-%d]*:[-%d]*:[-%d]*:[-%d]*:[-%d]*:[-%d]*):[-%d]*:[-%d]*:[-%d]*:[-%d]*:[-%d]*(.*)$", "%1%2")
  key = key:gsub(":[:0]*$", "")
  return (key:gsub("::+", function(m) return string.char(57 + #m) end))
end
function ADB:ForecastRecord(key, now)
  local cache = _G.NauctionsForecastCache
  now = now or GetServerTime()
  if type(cache) ~= "table" or cache.schema_version ~= 1 or cache.realm ~= GetRealmName() or cache.house ~= self:PlayerHouse()
    or not finite(cache.generated_at) or cache.generated_at > now + 300 or not finite(cache.expires_at)
    or cache.expires_at <= now or cache.expires_at <= cache.generated_at or cache.expires_at - cache.generated_at > 2700
    or type(cache.records) ~= "table" or #cache.records > 30 then return end
  for _, row in ipairs(cache.records) do
    if type(row) == "table" and row.item_key == key and row.schema_version == 1 and not row.demo
      and row.realm == cache.realm and row.house == cache.house and row.market_id
      and finite(row.generated_at) and row.generated_at <= now + 300 and finite(row.expires_at)
      and row.expires_at > now and row.expires_at <= cache.expires_at and row.expires_at > row.generated_at
      and row.expires_at - row.generated_at <= 2700 then return row end
  end
end
local function level(item, day)
  local previous
  for _, point in ipairs(item.points) do
    if finite(point[3]) and point[3] > 0 then
      if point[1] == day then return math.log(point[3]) end
      if previous and previous[1] < day and day < point[1] then
        local fraction = (day - previous[1]) / (point[1] - previous[1])
        return math.log(previous[3]) + (math.log(point[3]) - math.log(previous[3])) * fraction
      end
      previous = point
    end
  end
end
function ADB:BetaScenario(id, day, horizonDays)
  local item = self.betaReference and self.betaReference.items[id]
  if not item or not finite(day) or day < 0 or day > 17 then return end
  local start, finish = level(item, day), level(item, day + horizonDays)
  if not start or not finish then return end
  return (math.exp(finish - start) - 1) * 100
end
local function percent(value) return value and ("%+.1f%%"):format(math.abs(value) < 0.05 and 0 or value) or "insufficient data" end
function ADB:ForecastTooltip(tt, data)
  if not (self.db and self.db.tooltip) or not data or not data.id then return end
  if data.type and data.type ~= Enum.TooltipDataType.Item then return end
  local _, link = tt:GetItem()
  local key = self:ForecastItemKey(link)
  local row = key and self:ForecastRecord(key)
  local rolling = row and row.rolling
  local value = type(rolling) == "table" and amount(rolling.price)
  if value and (rolling.status == "baseline" or rolling.status == "experimental") then
    tt:AddDoubleLine(rolling.status == "baseline" and "24h median (unchanged baseline)" or "24h median (experimental)", self:Money(value), .7, .85, 1, 1, 1, 1)
    local low, high = amount(rolling.low), amount(rolling.high)
    if low and high then tt:AddDoubleLine("24h historical-error range", self:Money(low) .. " - " .. self:Money(high), .7, .85, 1, 1, 1, 1) end
  else
    tt:AddLine("24h forecast: " .. (rolling and rolling.reason or "no fresh export for this item and market"), .65, .7, .8, true)
  end
  if row then
    tt:AddLine(("Nauctions export: %dm ago; historical errors are not probabilities"):format(math.max(0, math.floor((GetServerTime() - row.generated_at) / 60))), .65, .7, .8, true)
    if IsAltKeyDown() and type(row.launch) == "table" then
      for _, estimate in ipairs(row.launch) do
        local target = type(estimate) == "table" and amount(estimate.price)
        if target and estimate.status == "beta_based" then
          tt:AddDoubleLine(("Launch outlook +%dd (beta-based)"):format(estimate.horizon_hours / 24), self:Money(target), 1, .8, .4, 1, 1, 1)
        end
      end
    end
  end
  if self:PlayerHouse() ~= "Horde" then return end
  local beta = self.betaReference and self.betaReference.items[data.id]
  if not beta then return end
  local last
  for _, p in ipairs(beta.points) do if finite(p[3]) and p[3] > 0 then last = p end end
  if last then
    tt:AddDoubleLine(("Beta Normal Horde median (Oct %d%s)"):format(3 + last[1], beta.variants_combined and ", all variants" or ""), self:Money(last[3]), 1, .8, .4, 1, 1, 1)
  end
  local day = self.sv and self.sv.nauctionsForecastDay or 0
  if IsAltKeyDown() then
    tt:AddLine(("Launch scenario: beta reference day %d (not reset age)"):format(day), 1, .8, .4, true)
    for _, horizon in ipairs({1, 3, 7}) do
      local result = percent(self:BetaScenario(data.id, day, horizon))
      if row and row.scenario_day == day and type(row.scenario) == "table" then
        for _, estimate in ipairs(row.scenario) do
          local target = type(estimate) == "table" and amount(estimate.price)
          if target and estimate.status == "scenario" and estimate.horizon_hours == horizon * 24 then result = result .. " (~" .. self:Money(target) .. ")" end
        end
      end
      tt:AddDoubleLine(("Scenario +%dd (unvalidated)"):format(horizon), result, 1, .8, .4, 1, 1, 1)
    end
    tt:AddLine("Between beta dates: log-linear assumption. No extrapolation.", .7, .7, .7, true)
  else tt:AddLine("Hold Alt: beta launch scenarios", .7, .7, .7) end
  tt:AddLine("Data from ForeverDB.net", .6, .6, .6)
end
if TooltipDataProcessor then
  TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, function(tt, data) ADB:ForecastTooltip(tt, data) end)
end
ADB:AddCommand("forecastday", function(self, rest)
  local day = tonumber(rest)
  if not day or day ~= math.floor(day) or day < 0 or day > 17 then
    self:Print("forecastday 0..17: choose a scenario day relative to Oct 3, 2026; this is not a beta reset date.")
    return
  end
  self.sv.nauctionsForecastDay = day
  self:Print("Beta launch scenario reference day %d. Hold Alt over an item. This changes no observed prices.", day)
end, "forecastday 0..17 - beta scenario reference day (Oct 3 = 0, not a verified reset)")
