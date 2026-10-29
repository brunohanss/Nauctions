-- Nauctions modifications, 2026. Shared recipe facts only; no character state.
-- Distributed with the AHDB fork under its accompanying licence and notices.
local addonName, ADB = ...
local P = { status = "Open a profession window to discover recipes.", generation = 0 }
ADB.professions = P
local MAX_RECIPES, BATCH = 20000, 10

local function positive(n) return type(n) == "number" and n > 0 and n == math.floor(n) end
local function call(fn, ...)
  if type(fn) ~= "function" then return nil end
  local ok, value = pcall(fn, ...)
  if ok then return value end
end

local function currentBuild()
  local version, build = GetBuildInfo()
  if not version or not version:match("^1%.6%d%.") then return nil end
  return version .. "." .. tostring(build)
end

function P:Initialize()
  local build = currentBuild()
  if not build then self.status = "Profession capture requires a supported Forever client."; return false end
  local saved = NauctionsProfessionsSaved
  if saved and saved.schema_version ~= 1 then
    self.status = "Unsupported saved profession schema; capture is paused."
    return false
  end
  -- A new build starts a new observation set. The server retains the old build.
  if not saved or saved.build ~= build or saved.locale ~= GetLocale() then
    saved = { schema_version = 1, product = "wow_classic_beta", build = build, recipes = {}, recipe_items = {} }
    NauctionsProfessionsSaved = saved
  end
  saved.addon_version = ADB.version
  saved.locale = GetLocale()
  saved.recipe_items = saved.recipe_items or {}
  if self.saved ~= saved then
    self.recipeCount = 0
    for _ in pairs(saved.recipes) do self.recipeCount = self.recipeCount + 1 end
  end
  self.saved = saved
  return true
end

function P:ReadRecipe(id)
  local api = C_TradeSkillUI
  if not api then return nil end
  local info = call(api.GetRecipeInfo, id)
  local schematic = call(api.GetRecipeSchematic, id, false)
  local profession = call(api.GetProfessionInfoByRecipeID, id)
  if not info or not schematic or not profession or not positive(profession.professionID)
      or not info.name or not profession.professionName then return nil end
  if schematic.recipeID ~= id or (info.recipeID and info.recipeID ~= id) then return nil end
  -- Never persist info.learned, skill rank, inventory counts, GUIDs or character names.
  if info.isDummyRecipe or info.isGatheringRecipe or info.isSalvageRecipe or info.supportsQualities then return nil end
  local service = info.isEnchantingRecipe or schematic.recipeType == 3
  local recipe = {
    profession_id = profession.professionID, profession_name = profession.professionName,
    name = info.name, captured_at = GetServerTime(), kind = service and "service" or "item",
    reagents = {}, tools = {}
  }
  local slots = schematic.reagentSlotSchematics
  if type(slots) ~= "table" then return nil end
  for _, slot in ipairs(slots) do
    local quantity = slot.quantityRequired
    if slot.required or (type(quantity) == "number" and quantity > 0) then
      -- Alternative reagents and currency costs need a separate model. Do not invent an input.
      if not positive(quantity) or not slot.reagents or #slot.reagents ~= 1 then return nil end
      local reagent = slot.reagents[1]
      if not positive(reagent.itemID) or reagent.currencyID then return nil end
      recipe.reagents[reagent.itemID] = (recipe.reagents[reagent.itemID] or 0) + quantity
    end
  end
  if not service then
    local output = call(api.GetRecipeOutputItemData, id)
    local itemID = schematic.outputItemID or (output and output.itemID)
    if not positive(itemID) then return nil end
    recipe.output_item_id = itemID
    if positive(schematic.quantityMin) and positive(schematic.quantityMax)
        and schematic.quantityMin <= schematic.quantityMax then
      recipe.min_output, recipe.max_output = schematic.quantityMin, schematic.quantityMax
    end
    if C_Item and C_Item.GetItemInfo then
      local binding = select(14, C_Item.GetItemInfo(itemID))
      if binding ~= nil then recipe.tradable = binding == 0 or binding == 2 or binding == 3 end
      if binding == nil and C_Item.RequestLoadItemDataByID then C_Item.RequestLoadItemDataByID(itemID) end
    end
  end
  local requirements = call(api.GetRecipeRequirements, id)
  for _, requirement in ipairs(requirements or {}) do
    if requirement.name then table.insert(recipe.tools, requirement.name) end
  end
  return recipe
end

function P:SaveRecipe(id, recipe)
  if not recipe then return false end
  if not self.saved.recipes[id] then
    if self.recipeCount >= MAX_RECIPES then return false end
    self.recipeCount = self.recipeCount + 1
  end
  self.saved.recipes[id] = recipe
  return true
end

function P:Capture(attempt)
  if not self:Initialize() then return end
  local api = C_TradeSkillUI
  if not api or not api.GetRecipeSchematic or not api.GetProfessionInfoByRecipeID then
    self.status = "The client does not expose the required recipe APIs."
    return
  end
  local ids = call(api.GetAllRecipeIDs) or call(api.GetFilteredRecipeIDs)
  if type(ids) ~= "table" then self.status = "Open a profession window, then run /ahdb professions scan."; return end
  self.generation = self.generation + 1
  local generation, index, skipped = self.generation, 1, 0
  local function step()
    if generation ~= self.generation then return end
    for _ = 1, BATCH do
      local id = ids[index]
      if not id then
        self.status = ("%d discovered recipes; %d unresolved or unsupported in this pass. Save with /reload or logout."):format(self.recipeCount, skipped)
        if skipped > 0 and (attempt or 0) < 2 then
          C_Timer.After(2, function() if generation == self.generation then self:Capture((attempt or 0) + 1) end end)
        end
        return
      end
      index = index + 1
      local recipe = positive(id) and self:ReadRecipe(id)
      if not self:SaveRecipe(id, recipe) then skipped = skipped + 1 end
    end
    C_Timer.After(0.05, step)
  end
  self.status = "Reading exposed recipe data..."
  step()
end

local pending = false
local frame = CreateFrame("Frame")
for _, event in ipairs({"TRADE_SKILL_SHOW", "TRADE_SKILL_LIST_UPDATE", "TRADE_SKILL_DATA_SOURCE_CHANGED"}) do
  pcall(frame.RegisterEvent, frame, event)
end
frame:SetScript("OnEvent", function()
  if pending then return end
  pending = true
  C_Timer.After(0.5, function() pending = false; P:Capture() end)
end)
ADB:AddCommand("professions", function(self, rest)
  if rest == "scan" then
    P:Capture()
    if self.auctionRecipes then self.auctionRecipes:Retry() end
  end
  self:Print("Nauctions professions: %s", P.status)
  if self.auctionRecipes then self:Print("Auction recipe items: %s", self.auctionRecipes:Status()) end
end, "professions [scan] - shared recipe capture status or rescan")
