-- Nauctions forecasts from live market history. The cache does not change observed history.
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
    if type(rolling.range_kind) == "string" and rolling.range_kind:find("^limited_history") then
      tt:AddLine("Provisional estimate: limited history; accuracy is not established.", .65, .7, .8, true)
    end
    local low, high = amount(rolling.low), amount(rolling.high)
    if low and high then tt:AddDoubleLine("24h historical-error range", self:Money(low) .. " - " .. self:Money(high), .7, .85, 1, 1, 1, 1) end
  else
    tt:AddLine("24h forecast: " .. (rolling and rolling.reason or "no fresh export for this item and market"), .65, .7, .8, true)
  end
  if row then
    tt:AddLine(("Nauctions export: %dm ago; historical errors are not probabilities"):format(math.max(0, math.floor((GetServerTime() - row.generated_at) / 60))), .65, .7, .8, true)
  end
end

if TooltipDataProcessor then
  TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, function(tt, data) ADB:ForecastTooltip(tt, data) end)
end
