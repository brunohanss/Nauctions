-- Nauctions modifications, 2026. Discover recipes from auction item IDs.
-- Uses read-only APIs. Never buys, uses, learns or searches for an item automatically.
local addonName, ADB = ...
local P = ADB.professions
local D = { queue = {}, queued = {}, attempts = {}, waiting = {}, waitingSpells = {}, itemSpell = {}, seen = {}, head = 1, running = false }
ADB.auctionRecipes = D
local BATCH, MAX_QUEUE, MAX_ITEMS, MAX_ATTEMPTS = 10, 50000, 20000, 3
local CLASSIFY_BATCH = 100
local function positive(n) return type(n) == "number" and n > 0 and n <= 1000000000 and n == math.floor(n) end
local function call(fn, ...)
  if type(fn) ~= "function" then return nil end
  local ok, result = pcall(fn, ...)
  if ok then return result end
end

local function itemClass(id)
  if not C_Item then return nil end
  if C_Item.GetItemInfoInstant then
    local ok, _, _, _, _, _, classID = pcall(C_Item.GetItemInfoInstant, id)
    if ok and classID ~= nil then return classID end
  end
  return call(function() return select(12, C_Item.GetItemInfo(id)) end)
end

-- Teaching-spell fields only. Never interpret localized text or an output item ID as a spell.
local function teachingSpells(id)
  local data = C_TooltipInfo and call(C_TooltipInfo.GetItemByID, id)
  local spells, count = {}, 0
  local types = Enum and Enum.TooltipDataLineType
  if data and type(data.lines) == "table" then
    for i = 1, math.min(#data.lines, 128) do
      local line = data.lines[i]
      if types and (line.type == types.ItemSpellTriggerLearn or line.type == types.LearnableSpell) then
        local spell = line.spellID
        if not positive(spell) and type(line.args) == "table" then
          for j = 1, math.min(#line.args, 32) do
            local arg = line.args[j]
            if arg.field == "spellID" and positive(arg.intVal) then spell = arg.intVal; break end
          end
        end
        if positive(spell) and not spells[spell] then spells[spell] = true; count = count + 1 end
      end
    end
  end
  if count == 1 then return next(spells), false end
  if count > 1 then return nil, true end
  -- Some clients expose the recipe directly here; others expose a use/learning wrapper.
  -- ReadRecipe checks the returned recipe IDs and full schematic before accepting it.
  local spell = C_Item and call(function() return select(2, C_Item.GetItemSpell(id)) end)
  return positive(spell) and spell or nil, false
end

function D:ResetForSave()
  if not P:Initialize() then return false end
  if self.saved ~= P.saved then
    self.saved = P.saved
    self.queue, self.queued, self.attempts, self.waiting, self.seen = {}, {}, {}, {}, {}
    self.waitingSpells, self.itemSpell, self.capacityReached = {}, {}, false
    self.head, self.seenCount = 1, 0
    self.itemCount = 0
    for _ in pairs(self.saved.recipe_items) do self.itemCount = self.itemCount + 1 end
  end
  return true
end

function D:SetWaitingSpell(id, spell)
  local previous = self.itemSpell[id]
  if previous and self.waitingSpells[previous] then
    self.waitingSpells[previous][id] = nil
    if not next(self.waitingSpells[previous]) then self.waitingSpells[previous] = nil end
  end
  self.itemSpell[id] = spell
  if spell then
    self.waitingSpells[spell] = self.waitingSpells[spell] or {}
    self.waitingSpells[spell][id] = true
  end
end

function D:ReadItem(id)
  self:SetWaitingSpell(id, nil)
  local class = itemClass(id)
  if class == nil then
    self.waiting[id] = true
    if C_Item then call(C_Item.RequestLoadItemDataByID, id) end
    return false, false
  end
  local recipeClass = Enum and Enum.ItemClass and Enum.ItemClass.Recipe or 9
  if class ~= recipeClass then self.waiting[id] = nil; return true, false end
  local item = self.saved.recipe_items[id]
  if not item then
    if self.itemCount >= MAX_ITEMS then self.capacityReached = true; return true, true end
    item = { captured_at = GetServerTime(), status = "mapping_unavailable" }
    self.saved.recipe_items[id] = item
    self.itemCount = self.itemCount + 1
  end
  local spell, ambiguous = teachingSpells(id)
  if ambiguous then item.status = "ambiguous_spell"; self.waiting[id] = nil; return true, true end
  if not spell then
    if item.status ~= "resolved" then item.status = "mapping_unavailable" end
    self.waiting[id] = true
    if C_Item then call(C_Item.RequestLoadItemDataByID, id) end
    return false, true
  end
  local recipe = P:ReadRecipe(spell)
  if recipe and P:SaveRecipe(spell, recipe) then
    item.recipe_spell_id, item.status = spell, "resolved"
    self.waiting[id] = nil
    return true, true
  end
  -- Do not erase a previously resolved link when APIs are temporarily unavailable.
  if item.status ~= "resolved" then item.recipe_spell_id, item.status = spell, "details_unavailable" end
  self.waiting[id] = true
  self:SetWaitingSpell(id, spell)
  if C_Spell then call(C_Spell.RequestLoadSpellData, spell) end
  return false, true
end

function D:Pump()
  if self.running then return end
  self.running = true
  local function step()
    if not self:ResetForSave() then self.running = false; return end
    local recipesRead = 0
    for _ = 1, CLASSIFY_BATCH do
      local id = self.queue[self.head]
      if not id then
        self.queue, self.head, self.running = {}, 1, false
        return
      end
      self.head = self.head + 1
      self.queued[id] = nil
      self.attempts[id] = (self.attempts[id] or 0) + 1
      local ok, complete, isRecipe = pcall(self.ReadItem, self, id)
      if not ok then self.waiting[id] = true end
      if (not ok or not complete) and self.attempts[id] < MAX_ATTEMPTS then
        C_Timer.After(2, function() self:Enqueue(id, false) end)
      end
      if isRecipe or not ok then recipesRead = recipesRead + 1 end
      if recipesRead >= BATCH then break end
    end
    C_Timer.After(0.05, step)
  end
  C_Timer.After(0, step)
end

function D:Enqueue(id, observed)
  if not positive(id) or not self:ResetForSave() then return end
  if not self.seen[id] then
    if self.seenCount >= MAX_QUEUE then self.capacityReached = true; return end
    self.seen[id], self.seenCount = true, self.seenCount + 1
  end
  if observed then
    -- Once per current scan/browse batch, not once per duplicate auction row.
    self.attempts[id] = 0
    local item = self.saved.recipe_items[id]
    if item then item.captured_at = GetServerTime() end
  end
  if self.queued[id] or (self.attempts[id] or 0) >= MAX_ATTEMPTS then return end
  self.queue[#self.queue + 1], self.queued[id] = id, true
  self:Pump()
end

function D:ObserveResults(results)
  local seen = {}
  for _, result in ipairs(results or {}) do
    local id = result.itemKey and result.itemKey.itemID
    if positive(id) and not seen[id] then seen[id] = true; self:Enqueue(id, true) end
  end
end

function D:Retry()
  if not self:ResetForSave() then return end
  local pending = {}
  for id in pairs(self.waiting) do pending[id] = true end
  for id, item in pairs(self.saved.recipe_items) do
    if item.status ~= "resolved" then pending[id] = true end
  end
  for id in pairs(pending) do self.attempts[id] = 0; self:Enqueue(id, false) end
end

function D:Status()
  if not self:ResetForSave() then return "capture unavailable" end
  local total, resolved = 0, 0
  for _, item in pairs(self.saved.recipe_items) do
    total = total + 1
    if item.status == "resolved" then resolved = resolved + 1 end
  end
  return ("%d discovered, %d resolved, %d pending%s. Save with /reload or logout."):format(total, resolved, total - resolved, self.capacityReached and " (capacity limit reached)" or "")
end

ADB:Listen("SCAN_DATA", function(_, e)
  local seen = {}
  for _, id in ipairs(e.item or {}) do
    if not seen[id] then seen[id] = true; D:Enqueue(id, true) end
  end
end)

local frame = CreateFrame("Frame")
for _, event in ipairs({"ADDON_LOADED", "AUCTION_HOUSE_BROWSE_RESULTS_UPDATED", "AUCTION_HOUSE_BROWSE_RESULTS_ADDED", "ITEM_SEARCH_RESULTS_UPDATED", "ITEM_SEARCH_RESULTS_ADDED", "GET_ITEM_INFO_RECEIVED", "SPELL_DATA_LOAD_RESULT", "TRADE_SKILL_SHOW", "TRADE_SKILL_LIST_UPDATE"}) do
  pcall(frame.RegisterEvent, frame, event)
end
local browsePending = false
local detailsPending = false
frame:SetScript("OnEvent", function(_, event, arg)
  if event == "ADDON_LOADED" then
    if arg == addonName then D:Retry() end
  elseif event == "AUCTION_HOUSE_BROWSE_RESULTS_ADDED" then
    D:ObserveResults(arg)
  elseif event == "AUCTION_HOUSE_BROWSE_RESULTS_UPDATED" then
    if browsePending then return end
    browsePending = true
    C_Timer.After(0.2, function()
      browsePending = false
      D:ObserveResults(C_AuctionHouse and call(C_AuctionHouse.GetBrowseResults))
    end)
  elseif event == "ITEM_SEARCH_RESULTS_UPDATED" or event == "ITEM_SEARCH_RESULTS_ADDED" then
    D:Enqueue(type(arg) == "table" and arg.itemID, true)
  elseif event == "GET_ITEM_INFO_RECEIVED" then
    if D.waiting[arg] then D:Enqueue(arg, false) end
  elseif event == "SPELL_DATA_LOAD_RESULT" then
    for id in pairs(D.waitingSpells[arg] or {}) do D:Enqueue(id, false) end
  elseif event == "TRADE_SKILL_SHOW" then
    D:Retry()
  elseif event == "TRADE_SKILL_LIST_UPDATE" then
    if detailsPending then return end
    detailsPending = true
    C_Timer.After(0.5, function()
      detailsPending = false
      for id in pairs(D.waiting) do D:Enqueue(id, false) end
    end)
  end
end)
