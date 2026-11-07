-- Nauctions crafting estimates. Character state stays in memory and is never exported.
local _, ADB = ...
local C = { rows = {}, generation = 0, page = 1, readyOnly = false, status = "Open your profession window, then refresh." }
ADB.crafting = C
local PAGE, BATCH = 10, 10
local function call(fn, ...)
  if type(fn) ~= "function" then return nil end
  local ok, value = pcall(fn, ...)
  if ok then return value end
end
local function positive(n) return type(n) == "number" and n > 0 and n < math.huge end
local function localProfession()
  local api = C_TradeSkillUI
  if not api then return false end
  if ProfessionsFrame and not ProfessionsFrame:IsShown() then return false end
  if api.IsTradeSkillReady and call(api.IsTradeSkillReady) ~= true then return false end
  if Professions and Professions.InLocalCraftingMode then return call(Professions.InLocalCraftingMode) == true end
  -- Fail closed when the client cannot distinguish another player's profession.
  if not api.IsTradeSkillLinked or not api.IsTradeSkillGuild or not api.IsTradeSkillGuildMember or not api.IsNPCCrafting then return false end
  return call(api.IsTradeSkillLinked) == false and call(api.IsTradeSkillGuild) == false
    and call(api.IsTradeSkillGuildMember) == false and call(api.IsNPCCrafting) == false
end

-- Use one actual scan. Daily summaries and an item's last sighting in older scans do not fill gaps.
function C:Market(house)
  local h = ADB.sv and ADB.sv.houses[house]
  local scan = h and h.scans[#h.scans]
  local ts = scan and scan[1]
  return { house = house, ts = ts, items = h and h.items or {}, stale = not ts or GetServerTime() - ts > 3600 }
end
function C:Price(id, market)
  if not market.ts then return nil end
  for point in (market.items[id] or ""):gmatch("%S+") do
    local ts, minimum, median, quantity = point:match("^([^,]+),([^,]+),([^,]+),([^,]+)$")
    if tonumber(ts) == market.ts then
      minimum, quantity = tonumber(minimum), tonumber(quantity)
      if positive(minimum) and positive(quantity) then return minimum, quantity end
    end
  end
end
function C:Quote(recipe, market)
  local q = { recipe = recipe, market = market, cost = 0, missing = {}, supply = true }
  for id, count in pairs(recipe.reagents) do
    local price, quantity = self:Price(id, market)
    if not price then q.missing[#q.missing + 1] = id
    else q.cost = q.cost + price * count; if quantity < count then q.supply = false end end
  end
  table.sort(q.missing)
  if #q.missing > 0 then q.cost = nil end
  q.unitAsk = recipe.output_item_id and self:Price(recipe.output_item_id, market)
  local yield = recipe.min_output
  if not market.ts then q.reason = "No auction scan."
  elseif recipe.kind ~= "item" or recipe.tradable == false then q.reason = "Service or bound output: cost only."
  elseif not positive(yield) or yield ~= recipe.max_output then q.reason = "Unknown or variable output quantity."
  elseif not q.cost then q.reason = "Missing material prices."
  elseif not q.supply then q.reason = "Insufficient observed material supply."
  elseif not q.unitAsk then q.reason = "No finished-item buyout in this scan."
  else
    q.units = yield
    q.gross = q.unitAsk * yield
    q.cut = market.house == "Neutral" and 15 or 5
    q.net = q.gross * (100 - q.cut) / 100
    q.profit = q.net - q.cost
    q.roi = q.cost > 0 and q.profit / q.cost * 100 or nil
  end
  return q
end
local function money(n)
  if n == nil then return "Unknown" end
  return (n < 0 and "-" or "") .. ADB:Money(math.floor(math.abs(n) + .5))
end

function C:AddTooltip(tt, id, recipe, market)
  recipe = recipe or (ADB.professions.saved and ADB.professions.saved.recipes[id]) or ADB.professions:ReadRecipe(id)
  if not recipe then tt:AddLine("Nauctions: recipe details unavailable.", 1, .7, .3); return end
  local q = self:Quote(recipe, market or self:Market(ADB:ViewHouse()))
  tt:AddLine(" ")
  tt:AddLine("Nauctions crafting - " .. q.market.house, .6, .9, 1)
  tt:AddDoubleLine("AH materials / craft", money(q.cost), 1, 1, 1, 1, 1, 1)
  local reagents = {}
  for item in pairs(recipe.reagents) do reagents[#reagents + 1] = item end
  table.sort(reagents)
  for i = 1, math.min(8, #reagents) do
    local item = reagents[i]
    local count, price = recipe.reagents[item], self:Price(item, q.market)
    local name = call(C_Item.GetItemNameByID, item) or ("Item " .. item)
    tt:AddDoubleLine(("  %d x %s"):format(count, name), money(price and price * count), .7, .7, .7, 1, 1, 1)
  end
  tt:AddDoubleLine("Finished item / unit (ask)", money(q.unitAsk), 1, 1, 1, 1, 1, 1)
  if q.gross then
    tt:AddDoubleLine(("Sale / craft (%d outputs)"):format(q.units), money(q.gross), 1, 1, 1, 1, 1, 1)
    tt:AddDoubleLine(("Sale after %d%% cut"):format(q.cut), money(q.net), 1, 1, 1, 1, 1, 1)
    tt:AddDoubleLine("Estimated profit / craft", money(q.profit), 1, 1, 1, q.profit >= 0 and .3 or 1, q.profit >= 0 and 1 or .3, .3)
    if q.roi then tt:AddDoubleLine("Return on material cost", ("%.1f%%"):format(q.roi)) end
  end
  if q.reason then tt:AddLine(q.reason, 1, .7, .3, true) end
  if #q.missing > 0 then
    local names = {}
    for i = 1, math.min(5, #q.missing) do names[i] = call(C_Item.GetItemNameByID, q.missing[i]) or ("Item " .. q.missing[i]) end
    tt:AddLine("No price: " .. table.concat(names, ", ") .. (#q.missing > 5 and " ..." or ""), 1, .7, .3, true)
  end
  if recipe.tradable == nil then tt:AddLine("Output binding is not verified.", 1, .7, .3) end
  if q.market.ts then tt:AddLine("Scan " .. ADB:Age(q.market.ts) .. " ago" .. (q.market.stale and " - STALE" or "") .. "; coverage not verified.", 1, .7, .3, true) end
  tt:AddLine("Unit-price estimate. Whole stacks and deposits excluded. Asking prices do not prove a sale.", .7, .7, .7, true)
end

function C:ItemTooltip(tt, data)
  if not (ADB.db and ADB.db.tooltip) or not data or not data.id then return end
  local saved = ADB.professions.saved
  if not saved then ADB.professions:Initialize(); saved = ADB.professions.saved end
  local item = saved and saved.recipe_items[data.id]
  if item then
    if item.status == "resolved" and item.recipe_spell_id then self:AddTooltip(tt, item.recipe_spell_id)
    else tt:AddLine("Nauctions: recipe details pending. Browse the AH or run /nauctions professions scan.", 1, .7, .3, true) end
  end
end

function C:VisibleRows()
  local rows = {}
  for _, row in ipairs(self.rows) do
    if not self.readyOnly or row.ready then rows[#rows + 1] = row end
  end
  return rows
end
function C:Refresh()
  self.generation = self.generation + 1
  local generation = self.generation
  self.rows, self.page = {}, 1
  if self.closed or not localProfession() then self.status = "Open your own profession window. Linked and NPC recipes are excluded."; self:Draw(); return end
  local api = C_TradeSkillUI
  local ids = call(api.GetAllRecipeIDs) or call(api.GetFilteredRecipeIDs)
  if type(ids) ~= "table" then self.status = "Recipe list unavailable. Open your profession window."; self:Draw(); return end
  local market, index, rows = self:Market(self.house or ADB:ViewHouse()), 1, {}
  self.status = "Reading known recipes..."; self:Draw()
  local function step()
    if generation ~= self.generation then return end
    if not localProfession() then self.rows = {}; self.status = "Open your own profession window, then refresh."; self:Draw(); return end
    for _ = 1, BATCH do
      local id = ids[index]
      if not id or index > 20000 then
        table.sort(rows, function(a, b)
          local ap, bp = a.quote.profit, b.quote.profit
          if (ap ~= nil) ~= (bp ~= nil) then return ap ~= nil end
          if ap ~= bp then return ap > bp end
          if a.recipe.name ~= b.recipe.name then return a.recipe.name < b.recipe.name end
          return a.id < b.id
        end)
        self.rows = rows
        self.status = ("%d known recipes in the open profession. %s"):format(#rows,
          market.ts and ("Scan " .. ADB:Age(market.ts) .. " ago" .. (market.stale and " - STALE." or ".")) or "No auction scan.")
        self:Draw(); return
      end
      index = index + 1
      local info = call(api.GetRecipeInfo, id)
      if info and info.learned == true then
        local recipe = ADB.professions:ReadRecipe(id)
        if recipe then
          local count = call(api.GetCraftableCount, id)
          local requirements = call(api.GetRecipeRequirements, id)
          local cooldown = Professions and call(Professions.IsRecipeOnCooldown, id)
          local ready = positive(count) and info.craftable == true and not info.disabled
            and cooldown == false and type(requirements) == "table"
          for _, r in ipairs(requirements or {}) do if r.met ~= true then ready = false end end
          rows[#rows + 1] = { id = id, recipe = recipe, quote = self:Quote(recipe, market), ready = ready == true, count = count }
        end
      end
    end
    C_Timer.After(.05, step)
  end
  step()
end

local function label(parent, text, x, y, width)
  local t = parent:CreateFontString(nil, "OVERLAY", "GameFontHighlightSmall")
  t:SetPoint("TOPLEFT", x, y); t:SetWidth(width); t:SetJustifyH("LEFT"); t:SetText(text)
  return t
end
local function button(parent, text, x, y, width, fn)
  local b = CreateFrame("Button", nil, parent, "UIPanelButtonTemplate")
  b:SetPoint("TOPLEFT", x, y); b:SetSize(width, 24); b:SetText(text); b:SetScript("OnClick", fn)
  return b
end
function C:Build()
  local f = CreateFrame("Frame", "NauctionsCraftingFrame", UIParent, "BackdropTemplate")
  self.frame = f
  f:SetSize(800, 520); f:SetPoint("CENTER"); f:SetFrameStrata("DIALOG"); f:SetClampedToScreen(true)
  f:SetBackdrop({ bgFile = "Interface\\DialogFrame\\UI-DialogBox-Background", edgeFile = "Interface\\DialogFrame\\UI-DialogBox-Border", tile = true, tileSize = 32, edgeSize = 32, insets = { left = 11, right = 11, top = 11, bottom = 11 } })
  f:SetMovable(true); f:EnableMouse(true); f:RegisterForDrag("LeftButton")
  f:SetScript("OnDragStart", f.StartMoving); f:SetScript("OnDragStop", f.StopMovingOrSizing)
  CreateFrame("Button", nil, f, "UIPanelCloseButton"):SetPoint("TOPRIGHT", -5, -5)
  table.insert(UISpecialFrames, "NauctionsCraftingFrame")
  label(f, "Nauctions - Crafting profit", 22, -22, 600)
  for i, house in ipairs({ "Alliance", "Horde", "Neutral" }) do
    button(f, house, 22 + (i - 1) * 85, -48, 80, function() self.house = house; self:Refresh() end)
  end
  self.filter = button(f, "Known (buy materials)", 290, -48, 180, function()
    self.readyOnly = not self.readyOnly; self.page = 1; self:Draw()
  end)
  button(f, "Refresh", 480, -48, 90, function() self:Refresh() end)
  self.statusLabel = label(f, "", 22, -83, 750)
  label(f, "Recipe / state", 22, -115, 270)
  label(f, "AH cost / craft", 302, -115, 140)
  label(f, "Sale / craft (ask)", 452, -115, 140)
  label(f, "Profit after cut", 602, -115, 165)
  self.buttons = {}
  for i = 1, PAGE do
    local b = CreateFrame("Button", nil, f)
    b:SetPoint("TOPLEFT", 22, -135 - (i - 1) * 29); b:SetSize(750, 29)
    b:SetHighlightTexture("Interface\\QuestFrame\\UI-QuestTitleHighlight")
    b.name = label(b, "", 0, -2, 270); b.cost = label(b, "", 280, -2, 145)
    b.sale = label(b, "", 430, -2, 145); b.profit = label(b, "", 580, -2, 165)
    b:SetScript("OnEnter", function(owner)
      local row = owner.row
      if not row then return end
      GameTooltip:SetOwner(owner, "ANCHOR_RIGHT"); GameTooltip:SetText(row.recipe.name)
      self:AddTooltip(GameTooltip, row.id, row.recipe, row.quote.market); GameTooltip:Show()
    end)
    b:SetScript("OnLeave", GameTooltip_Hide)
    b:SetScript("OnClick", function(owner) if owner.row and owner.row.recipe.output_item_id then ADB:ShowItem(owner.row.recipe.output_item_id) end end)
    self.buttons[i] = b
  end
  button(f, "Previous", 22, -433, 85, function() self.page = math.max(1, self.page - 1); self:Draw() end)
  button(f, "Next", 115, -433, 85, function() self.page = math.min(math.max(1, math.ceil(#self:VisibleRows() / PAGE)), self.page + 1); self:Draw() end)
  self.pageLabel = label(f, "", 215, -440, 540)
  label(f, "Sorted by profit per craft. Hover for details; click for output price history.", 22, -470, 750)
  label(f, "Ready checks materials, requirements and cooldown. AH supply and sales are not guaranteed.", 22, -490, 750)
end
function C:Draw()
  if not self.frame then return end
  self.filter:SetText(self.readyOnly and "Ready now" or "Known (buy materials)")
  self.statusLabel:SetText((self.house or ADB:ViewHouse()) .. " - " .. self.status)
  local rows = self:VisibleRows()
  self.pageLabel:SetText(#rows == 0 and "No matching recipes. Open your profession, check the filter, then refresh."
    or ("Page %d / %d - %d recipes"):format(self.page, math.max(1, math.ceil(#rows / PAGE)), #rows))
  for i, b in ipairs(self.buttons) do
    local row = rows[(self.page - 1) * PAGE + i]
    b.row = row; b:SetShown(row ~= nil)
    if row then
      b.name:SetText(row.recipe.name .. (row.ready and " |cFF66FF66[Ready]|r" or " [Needs materials/check]"))
      b.cost:SetText(money(row.quote.cost)); b.sale:SetText(money(row.quote.gross))
      local p = row.quote.profit
      b.profit:SetText(p and ((p >= 0 and "|cFF66FF66" or "|cFFFF6666") .. money(p) .. "|r") or "Unavailable (hover)")
    end
  end
end
function C:Show()
  if not self.frame then self:Build() end
  self.frame:Show(); self:Refresh()
end
ADB:AddCommand("crafting", function() C:Show() end, "crafting - known recipe costs, auction asks and profit")

-- Keep a visible entry point beside the profession window without replacing native controls.
local function professionButton()
  if not C.resultTooltipHooked and hooksecurefunc and GameTooltip
      and type(GameTooltip.SetRecipeResultItem) == "function" then
    C.resultTooltipHooked = true
    -- The native output icon rebuilds its result tooltip on every OnUpdate.
    -- Append after each rebuild, rather than adding lines only on OnEnter.
    hooksecurefunc(GameTooltip, "SetRecipeResultItem", function(tt, id)
      if not (ADB.db and ADB.db.tooltip) or not positive(id)
          or not ProfessionsFrame or not ProfessionsFrame:IsShown() then return end
      C:AddTooltip(tt, id)
      tt:Show()
    end)
  end
  if not ProfessionsFrame then return end
  if C.openButton then return end
  C.openButton = button(ProfessionsFrame, "Crafting profit", 0, 0, 140, function() C:Show() end)
  C.openButton:ClearAllPoints()
  C.openButton:SetPoint("BOTTOMLEFT", ProfessionsFrame, "TOPLEFT", 20, 2)
end

-- The normal profession event supplies elementData, not a recipe-item ID.
if EventRegistry then
  EventRegistry:RegisterCallback("Professions.RecipeListOnEnter", function(_, owner, data)
    local info = data and data.recipeInfo
    if not (ADB.db and ADB.db.tooltip) or not owner or not info or not info.recipeID then return end
    if not C.tip then C.tip = CreateFrame("GameTooltip", "NauctionsCraftingTooltip", UIParent, "GameTooltipTemplate") end
    C.tip:SetOwner(owner, "ANCHOR_RIGHT"); C.tip:SetText(info.name)
    C:AddTooltip(C.tip, info.recipeID); C.tip:Show()
    if not owner.nauctionsCraftingLeave then
      owner.nauctionsCraftingLeave = true
      owner:HookScript("OnLeave", function() C.tip:Hide() end)
      owner:HookScript("OnHide", function() C.tip:Hide() end)
    end
  end, C)
end
if TooltipDataProcessor then
  TooltipDataProcessor.AddTooltipPostCall(Enum.TooltipDataType.Item, function(tt, data) C:ItemTooltip(tt, data) end)
end
local pending = false
local events = CreateFrame("Frame")
for _, event in ipairs({ "ADDON_LOADED", "TRADE_SKILL_SHOW", "TRADE_SKILL_LIST_UPDATE", "TRADE_SKILL_DATA_SOURCE_CHANGED", "TRADE_SKILL_CLOSE", "BAG_UPDATE_DELAYED", "SPELL_UPDATE_COOLDOWN" }) do
  pcall(events.RegisterEvent, events, event)
end
events:SetScript("OnEvent", function(_, event)
  if event == "TRADE_SKILL_SHOW" then C.closed = false end
  if event == "TRADE_SKILL_CLOSE" then C.closed = true end
  if event == "ADDON_LOADED" or event == "TRADE_SKILL_SHOW" then professionButton() end
  if event == "ADDON_LOADED" then return end
  if event == "TRADE_SKILL_CLOSE" or event == "TRADE_SKILL_DATA_SOURCE_CHANGED" then
    C.generation = C.generation + 1; C.rows = {}; C.status = "Open your own profession window, then refresh."; C:Draw()
    if C.tip then C.tip:Hide() end
  end
  if pending or not C.frame or not C.frame:IsShown() then return end
  pending = true
  C_Timer.After(.75, function() pending = false; C:Refresh() end)
end)
ADB:Listen("SCAN_DATA", function() if C.frame and C.frame:IsShown() then C:Refresh() end end)
ADB:Listen("HISTORY_MERGED", function() if C.frame and C.frame:IsShown() then C:Refresh() end end)
ADB:Listen("LOGIN", professionButton)
