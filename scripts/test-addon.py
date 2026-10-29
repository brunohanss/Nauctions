"""Execute trusted addon sources in a simulated client; never load user SavedVariables."""
from pathlib import Path
import json
import hashlib
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "artifacts/lua-test-runtime"))
from lupa.lua51 import LuaRuntime


class AddonTests(unittest.TestCase):
    def test_unmodified_upstream_sources_match_pinned_git_blobs(self):
        manifest = json.loads((ROOT / "addon/AuctionDB/UPSTREAM.json").read_text())
        changed = {"AuctionDB/AuctionDB.lua", "AuctionDB/AuctionDB.toc", "AuctionDB/Minimap.lua", "AuctionDB/Record.lua", "AuctionDB/Scan.lua"}
        for entry in manifest["upstreamFiles"]:
            if entry["path"] in changed:
                continue
            data = (ROOT / "addon" / entry["path"]).read_bytes().replace(b"\r\n", b"\n")
            blob = b"blob " + str(len(data)).encode() + b"\0" + data
            self.assertEqual(hashlib.sha1(blob).hexdigest(), entry["gitBlob"], entry["path"])

    def record_client(self):
        lua = LuaRuntime(unpack_returned_tuples=True)
        lua.execute("""
            ADB = {sv={}, db={keepScans=3}, scanHouseName='Alliance', scanStart=0, ahOpen=true}
            function ADB:Listen() end
            function ADB:AddCommand() end
            function ADB:Debug() end
            function ADB:Print() end
            function GetLocale() return 'enGB' end
            function GetServerTime() return 100 end
            function GetTime() return 1 end
            function GetRealmName() return 'Synthetic Realm' end
            function GetUnitName() return 'Synthetic' end
            C_AuctionHouse = { GetReplicateItemLink=function() return nil end }
            C_Item = { GetItemInfo=function() return nil, '|cffffffff|Hitem:123:0:0:0:0:0:0:0:0:0:0:0|h[Base]|h|r' end }
            C_Timer = {After=function(_, fn) fn() end}
        """)
        lua.execute((ROOT / "addon/AuctionDB/Record.lua").read_text(), "AuctionDB", lua.globals().ADB)
        return lua

    def test_unresolved_variant_never_falls_back_to_base_item(self):
        lua = self.record_client()
        lua.execute("""
            local e={n=1, expectedCount=1, item={123}, idx={0}, link={}, count={1}, allTimeLeft={1}, minBid={0}, buyout={50}, curBid={0}}
            ADB:CompleteLinks(e, function() ADB:RecordScan(e) end)
            assert(e.link[1] == nil)
            assert(ADB.sv.ah[1].data == '')
            assert(ADB.sv.ah[1].count == 1)
            assert(ADB.sv.ah[1].itemsCount == 0)
        """)

    def test_record_preserves_expected_count_after_missing_replicate_row(self):
        lua = self.record_client()
        lua.execute("function ADB:On() end")
        lua.execute((ROOT / "addon/AuctionDB/Scan.lua").read_text(), "AuctionDB", lua.globals().ADB)
        lua.execute("""
            C_Item.GetItemInfo=function(link) return nil,nil,nil end
            C_AuctionHouse.GetNumReplicateItems=function() return 2 end
            C_AuctionHouse.GetReplicateItemInfo=function(idx)
                if idx == 1 then return nil end
                return 'Synthetic',nil,1,nil,nil,nil,nil,0,0,50,0,nil,nil,nil,nil,nil,123
            end
            C_AuctionHouse.GetReplicateItemLink=function(idx)
                return '|cffffffff|Hitem:123:0:0:0:0:0:0:0:0:0:0:0|h[Base]|h|r'
            end
            C_AuctionHouse.GetReplicateItemTimeLeft=function() return 1 end
            ADB.db.recordScans=true
            function ADB:RecordHistory() end
            function ADB:ScanDone(e) self:RecordScan(e) end
            function ADB:ScanFailed(e, err) error(err) end
            ADB:ReadReplicate()
            assert(ADB.sv.ah[1].count == 2)
            assert(ADB.sv.ah[1].data == 'i123!/2,1,,50,')
        """)

    def client(self):
        lua = LuaRuntime(unpack_returned_tuples=True)
        lua.execute(r'''
            timers = {}
            frames = {}
            C_Timer = { After = function(_, fn) table.insert(timers, fn) end }
            function drain()
              local count = 0
              while #timers > 0 do
                count = count + 1
                assert(count < 1000, "timer loop")
                table.remove(timers, 1)()
              end
            end
            function GetBuildInfo() return "1.60.1", "70630" end
            function GetLocale() return "enUS" end
            function GetServerTime() return 1793270000 end
            function CreateFrame()
              local frame = { events = {}, RegisterEvent = function(self, event) self.events[event] = true end, SetScript = function(self, _, fn) self.event = fn end }
              table.insert(frames, frame)
              return frame
            end
            function emit(event, ...)
              for _, frame in ipairs(frames) do if frame.events[event] then frame.event(frame, event, ...) end end
            end
            ADB = { version = "0.5.0-nauctions.2", commands = {}, listeners = {} }
            function ADB:AddCommand(name, fn) self.commands[name] = fn end
            function ADB:Listen(name, fn) self.listeners[name] = self.listeners[name] or {}; table.insert(self.listeners[name], fn) end
            function ADB:Fire(name, ...) for _, fn in ipairs(self.listeners[name] or {}) do fn(self, ...) end end
            function ADB:Print(...) end
            C_Item = { GetItemInfo = function() return nil,nil,nil,nil,nil,nil,nil,nil,nil,nil,nil,nil,nil,2 end }
            ids = { 900001, 900002 }
            C_TradeSkillUI = {
              GetAllRecipeIDs = function() return ids end,
              GetRecipeInfo = function(id) return { recipeID = id, name = "Synthetic " .. id, learned = id == 900001, isEnchantingRecipe = id == 900002 } end,
              GetProfessionInfoByRecipeID = function(id) return { professionID = 171, professionName = "Synthetic Alchemy", skillLevel = 300 } end,
              GetRecipeSchematic = function(id) return { recipeID = id, quantityMin = 2, quantityMax = 2, outputItemID = 900101, reagentSlotSchematics = { { required = true, quantityRequired = 3, reagents = { { itemID = 900201 } } } } } end,
              GetRecipeRequirements = function() return { { name = "Station", met = false } } end,
            }
        ''')
        lua.execute((ROOT / "addon/AuctionDB/NauctionsProfessions.lua").read_text(), "AuctionDB", lua.globals().ADB)
        return lua

    def crafting_client(self):
        lua = self.auction_client()
        lua.execute(r'''
          ADB.db={tooltip=true}
          ids={900001,900002}
          ADB.sv={houses={Alliance={scans={{1793270000,2,2}}, items={
            [900201]='1793270000,100,150,50', [900101]='1793270000,500,600,10'
          }}}}
          function ADB:ViewHouse() return 'Alliance' end
          function ADB:Age(ts) return tostring(GetServerTime()-ts)..'s' end
          function ADB:Money(n) return tostring(n)..'c' end
          function ADB:ShowItem(id) self.shownItem=id end
          Professions={InLocalCraftingMode=function() return true end, IsRecipeOnCooldown=function() return false end}
          local oldInfo=C_TradeSkillUI.GetRecipeInfo
          C_TradeSkillUI.GetRecipeInfo=function(id) local i=oldInfo(id); i.craftable=true; return i end
          C_TradeSkillUI.GetCraftableCount=function() return 2 end
          C_TradeSkillUI.GetRecipeRequirements=function() return {} end
          C_Item.GetItemNameByID=function(id) return 'Material '..id end
          callbacks={}
          EventRegistry={RegisterCallback=function(_, name, fn, owner) callbacks[name]={fn=fn,owner=owner} end}
          Enum.TooltipDataType={Item=0}
          TooltipDataProcessor={AddTooltipPostCall=function(_,fn) itemTooltip=fn end}
          UIParent={}; UISpecialFrames={}
          function GameTooltip_Hide() GameTooltip:Hide() end
          local function widget()
            local w={shown=true,scripts={},lines={},hooks={}}
            local function noop() end
            for _,name in ipairs({'SetPoint','ClearAllPoints','SetWidth','SetHeight','SetSize','SetJustifyH',
              'SetFrameStrata','SetClampedToScreen','SetBackdrop','SetMovable','EnableMouse','RegisterForDrag',
              'StartMoving','StopMovingOrSizing','SetHighlightTexture'}) do w[name]=noop end
            function w:SetText(text) self.text=text; self.lines={} end
            function w:SetOwner(owner,anchor) self.owner=owner end
            function w:AddLine(text) table.insert(self.lines,text) end
            function w:AddDoubleLine(left,right) table.insert(self.lines,left..'='..right) end
            function w:Show() self.shown=true end
            function w:Hide() self.shown=false; if self.scripts.OnHide then self.scripts.OnHide(self) end end
            function w:IsShown() return self.shown end
            function w:SetShown(shown) self.shown=shown end
            function w:SetScript(name,fn) self.scripts[name]=fn end
            function w:HookScript(name,fn) self.hooks[name]=fn end
            function w:RegisterEvent(event) self.events=self.events or {}; self.events[event]=true end
            function w:CreateFontString() return widget() end
            return w
          end
          function CreateFrame(_,name)
            local w=widget(); table.insert(frames,w); if name then _G[name]=w end; return w
          end
          GameTooltip=widget()
          function emitCraft(event)
            for _,f in ipairs(frames) do
              if f.events and f.events[event] then
                if f.scripts then f.scripts.OnEvent(f,event) else f.event(f,event) end
              end
            end
          end
        ''')
        lua.execute((ROOT / "addon/AuctionDB/NauctionsCrafting.lua").read_text(), "AuctionDB", lua.globals().ADB)
        return lua

    def test_crafting_quote_uses_one_scan_yield_and_auction_cut(self):
        lua = self.crafting_client()
        lua.execute('''
          local c=ADB.crafting
          local r=ADB.professions:ReadRecipe(900001)
          local m=c:Market('Alliance')
          local q=c:Quote(r,m)
          assert(q.cost==300 and q.units==2 and q.unitAsk==500)
          assert(q.gross==1000 and q.net==950 and q.profit==650)
          m.house='Neutral'
          q=c:Quote(r,m); assert(q.net==850 and q.profit==550)
          m.items[900201]='1793269000,1,1,100'
          q=c:Quote(r,m); assert(q.cost==nil and q.profit==nil and #q.missing==1)
          m.items[900201]='1793270000,0,0,50'
          q=c:Quote(r,m); assert(q.cost==nil and q.profit==nil)
          m.items[900201]='1793270000,100,100,2'
          q=c:Quote(r,m); assert(q.cost==300 and q.profit==nil and not q.supply)
        ''')

    def test_crafting_incomplete_bound_variable_and_negative_quotes(self):
        lua = self.crafting_client()
        lua.execute('''
          local c=ADB.crafting
          local r=ADB.professions:ReadRecipe(900001)
          local m=c:Market('Alliance')
          r.tradable=false
          local q=c:Quote(r,m); assert(q.cost==300 and q.profit==nil)
          r.tradable=true; r.max_output=3
          q=c:Quote(r,m); assert(q.profit==nil)
          r.max_output=2; m.items[900101]=nil
          q=c:Quote(r,m); assert(q.profit==nil and q.unitAsk==nil)
          m.items[900101]='1793270000,100,100,10'
          q=c:Quote(r,m); assert(q.profit==-110)
          q=c:Quote(r,c:Market('Horde')); assert(q.profit==nil and q.cost==nil)
          assert(c:Market('Horde').stale)
          ADB.sv.houses.Alliance.scans={{1793260000,1,1}}
          assert(c:Market('Alliance').stale)
        ''')

    def test_crafting_lists_only_known_recipes_and_checks_readiness(self):
        lua = self.crafting_client()
        lua.execute('''
          local c=ADB.crafting
          c:Refresh(); drain()
          assert(#c.rows==1 and c.rows[1].id==900001 and c.rows[1].ready)
          assert(c.rows[1].quote.profit==650)
          assert(NauctionsProfessionsSaved==nil, 'character state must not enter shared saved data')
          C_TradeSkillUI.GetRecipeRequirements=function() return {{met=false}} end
          c:Refresh(); drain(); assert(not c.rows[1].ready)
          c.readyOnly=true; assert(#c:VisibleRows()==0)
          C_TradeSkillUI.GetRecipeRequirements=function() return {} end
          Professions.IsRecipeOnCooldown=function() return true end
          c:Refresh(); drain(); assert(not c.rows[1].ready)
          Professions.IsRecipeOnCooldown=nil
          c:Refresh(); drain(); assert(not c.rows[1].ready)
          Professions.InLocalCraftingMode=function() return false end
          c:Refresh(); drain(); assert(#c.rows==0)
        ''')

    def test_crafting_sorts_profit_pages_and_opens_output_history(self):
        lua = self.crafting_client()
        lua.execute('''
          ids={}; for id=900010,900022 do ids[#ids+1]=id end
          C_TradeSkillUI.GetRecipeInfo=function(id) return {recipeID=id,name='Recipe '..id,learned=true,craftable=true} end
          local read=ADB.professions.ReadRecipe
          ADB.professions.ReadRecipe=function(self,id)
            local r=read(self,id); r.reagents[900201]=id-900009; return r
          end
          ADB.commands.crafting(); drain()
          local c=ADB.crafting
          assert(c.frame:IsShown() and #c.rows==13)
          assert(c.rows[1].id==900010 and c.rows[13].id==900022)
          assert(c.buttons[1].row.id==900010)
          c.page=2; c:Draw(); assert(c.buttons[1].row.id==900020 and not c.buttons[4]:IsShown())
          c.buttons[1].scripts.OnEnter(c.buttons[1]); assert(GameTooltip:IsShown())
          assert(table.concat(GameTooltip.lines,';'):find('AH materials / craft=1100c',1,true))
          c.buttons[1].scripts.OnClick(c.buttons[1]); assert(ADB.shownItem==900101)
          emitCraft('TRADE_SKILL_CLOSE'); assert(#c.rows==0)
        ''')

    def test_crafting_recipe_item_and_profession_hover_tooltips(self):
        lua = self.crafting_client()
        lua.execute('''
          ADB.professions:Initialize()
          ADB.professions:SaveRecipe(900001,ADB.professions:ReadRecipe(900001))
          NauctionsProfessionsSaved.recipe_items[910001]={status='resolved',recipe_spell_id=900001}
          itemTooltip(GameTooltip,{id=910001})
          local text=table.concat(GameTooltip.lines,';')
          assert(text:find('AH materials / craft=300c',1,true))
          assert(text:find('Finished item / unit (ask)=500c',1,true))
          assert(text:find('Estimated profit / craft=650c',1,true))
          assert(text:find('3 x Material 900201=300c',1,true))
          local cb=callbacks['ProfessionsDebug.CraftingRecipeListRecipeEntered']
          local owner=CreateFrame('Button')
          cb.fn(cb.owner,owner,{recipeID=900001,name='Synthetic'})
          assert(ADB.crafting.tip:IsShown() and ADB.crafting.tip.owner==owner)
          owner.hooks.OnLeave(); assert(not ADB.crafting.tip:IsShown())
          NauctionsProfessionsSaved.recipe_items[910002]={status='mapping_unavailable'}
          GameTooltip:SetText('Pending'); itemTooltip(GameTooltip,{id=910002})
          assert(table.concat(GameTooltip.lines,';'):find('recipe details pending',1,true))
          ADB.db.tooltip=false; GameTooltip:SetText('Off'); itemTooltip(GameTooltip,{id=910001})
          assert(#GameTooltip.lines==0)
        ''')

    def test_crafting_profession_button_and_close_cancel_pending_refresh(self):
        lua = self.crafting_client()
        lua.execute('''
          ProfessionsFrame=CreateFrame('Frame')
          emitCraft('ADDON_LOADED')
          local c=ADB.crafting
          assert(c.openButton and c.openButton.text=='Crafting profit')
          c.openButton.scripts.OnClick(); drain()
          assert(c.frame:IsShown() and #c.rows==1)
          emitCraft('BAG_UPDATE_DELAYED')
          emitCraft('TRADE_SKILL_CLOSE'); drain()
          assert(#c.rows==0, 'delayed refresh must not restore rows after close')
          emitCraft('TRADE_SKILL_SHOW'); drain()
          assert(#c.rows==1)
          ProfessionsFrame:Hide(); c:Refresh(); drain(); assert(#c.rows==0)
          ProfessionsFrame:Show()
          Professions=nil
          C_TradeSkillUI.IsTradeSkillLinked=function() return false end
          C_TradeSkillUI.IsTradeSkillGuild=function() return false end
          C_TradeSkillUI.IsNPCCrafting=function() return false end
          c:Refresh(); drain(); assert(#c.rows==0, 'unknown source must fail closed')
          C_TradeSkillUI.IsTradeSkillGuildMember=function() return true end
          c:Refresh(); drain(); assert(#c.rows==0, 'guild member recipes are not this character')
        ''')

    def test_all_addon_sources_compile_and_toc_is_complete(self):
        lua = LuaRuntime()
        toc = (ROOT / "addon/AuctionDB/AuctionDB.toc").read_text()
        for name in toc.splitlines():
            if name.endswith(".lua"):
                source = (ROOT / "addon/AuctionDB" / name).read_text()
                lua.execute("assert(loadstring(...))", source)
        self.assertIn("NauctionsProfessionsSaved", toc)
        self.assertIn("AuctionDBForeverSaved", toc)

    def test_capture_has_facts_for_both_learned_and_unlearned_recipes(self):
        lua = self.client()
        lua.execute("ADB.professions:Capture(); drain()")
        lua.execute('''
          local saved = NauctionsProfessionsSaved
          assert(saved.recipes[900001].reagents[900201] == 3)
          assert(saved.recipes[900001].min_output == 2)
          assert(saved.recipes[900001].tradable == true)
          assert(saved.recipes[900002].kind == "service")
          assert(saved.recipes[900002].output_item_id == nil)
          assert(saved.recipes[900001].learned == nil)
          assert(saved.recipes[900002].learned == nil)
          assert(saved.recipes[900001].skillLevel == nil)
        ''')
        output = ROOT / "artifacts/addon-test-export.lua"
        output.parent.mkdir(exist_ok=True)
        output.write_text("NauctionsProfessionsSaved=" + self.serialize(lua.globals().NauctionsProfessionsSaved), encoding="utf-8")

    def test_retry_does_not_replace_a_recipe_with_incomplete_data(self):
        lua = self.client()
        lua.execute('''
          ADB.professions:Capture(); drain()
          C_TradeSkillUI.GetRecipeSchematic = function() return nil end
          ADB.professions:Capture(); drain()
          assert(NauctionsProfessionsSaved.recipes[900001].min_output == 2)
        ''')

    def test_detects_new_builds_without_relabeling_previous_recipe_facts(self):
        lua = self.client()
        lua.execute('''
          ADB.professions:Capture(); drain()
          local original = NauctionsProfessionsSaved
          assert(original.build == "1.60.1.70630")
          assert(original.recipes[900001] ~= nil)
          assert(ADB.professions:Initialize())
          assert(NauctionsProfessionsSaved == original)
          GetBuildInfo = function() return "1.60.1", "70685" end
          assert(ADB.professions:Initialize())
          assert(NauctionsProfessionsSaved.build == "1.60.1.70685")
          assert(next(NauctionsProfessionsSaved.recipes) == nil)
          ADB.professions:Capture(); drain()
          assert(NauctionsProfessionsSaved.recipes[900001] ~= nil)
          GetBuildInfo = function() return "1.60.2", "71001" end
          assert(ADB.professions:Initialize())
          assert(NauctionsProfessionsSaved.build == "1.60.2.71001")
          assert(next(NauctionsProfessionsSaved.recipes) == nil)
        ''')

    def test_batches_alternative_inputs_and_build_changes(self):
        lua = self.client()
        lua.execute('''
          ids = {}
          for i = 1, 35 do ids[i] = i end
          ADB.professions:Capture()
          assert(NauctionsProfessionsSaved.recipes[11] == nil)
          drain()
          assert(NauctionsProfessionsSaved.recipes[35] ~= nil)
          GetBuildInfo = function() return "1.60.1", "70658" end
          ids = { 900003 }
          C_TradeSkillUI.GetRecipeSchematic = function(id) return { recipeID = id, reagentSlotSchematics = { { required = true, quantityRequired = 1, reagents = { {itemID=1}, {itemID=2} } } } } end
          ADB.professions:Capture(); drain()
          assert(NauctionsProfessionsSaved.build == "1.60.1.70658")
          assert(next(NauctionsProfessionsSaved.recipes) == nil)
        ''')

    @staticmethod
    def serialize(value):
        if hasattr(value, "items"):
            return "{" + ",".join("[" + AddonTests.serialize(k) + "]=" + AddonTests.serialize(v) for k, v in value.items()) + "}"
        if isinstance(value, bool):
            return "true" if value else "false"
        if isinstance(value, str):
            return json.dumps(value)
        return str(value)

    def auction_client(self):
        lua = self.client()
        lua.execute('''
          Enum = { ItemClass = { Recipe = 9 }, TooltipDataLineType = { ItemSpellTriggerLearn = 38, LearnableSpell = 6 } }
          classes, tooltips, requests = {}, {}, {}
          C_Item.GetItemInfoInstant = function(id) return id,nil,nil,nil,nil,classes[id] end
          C_Item.GetItemSpell = function() return "Use wrapper",999999 end
          C_Item.RequestLoadItemDataByID = function(id) requests[id] = (requests[id] or 0) + 1 end
          C_TooltipInfo = { GetItemByID = function(id) return tooltips[id] end }
          C_AuctionHouse = { GetBrowseResults = function() return browse or {} end }
          ids = {} -- No profession window or learned-recipe enumeration is needed.
        ''')
        lua.execute((ROOT / "addon/AuctionDB/NauctionsAuctionRecipes.lua").read_text(), "AuctionDB", lua.globals().ADB)
        return lua

    def test_auction_browse_discovers_unlearned_recipe_and_ignores_use_wrapper(self):
        lua = self.auction_client()
        lua.execute('''
          classes[910001], classes[910002] = 9,7
          tooltips[910001] = { lines = { { type = 38, args = { {field="spellID",intVal=900003} }, leftText="localized text is ignored" } } }
          browse = { {itemKey={itemID=910001}}, {itemKey={itemID=910001}}, {itemKey={itemID=910002}} }
          emit("AUCTION_HOUSE_BROWSE_RESULTS_UPDATED"); drain()
          local saved = NauctionsProfessionsSaved
          assert(saved.recipes[900003].min_output == 2)
          assert(saved.recipes[900003].learned == nil)
          assert(saved.recipes[999999] == nil)
          assert(saved.recipe_items[910001].recipe_spell_id == 900003)
          assert(saved.recipe_items[910001].status == "resolved")
          assert(saved.recipe_items[910002] == nil)
          assert(ADB.auctionRecipes.attempts[910001] == 1)
          classes[910003] = 9
          tooltips[910003] = { lines = { {type=38,spellID=900004} } }
          C_TradeSkillUI.GetRecipeInfo = function() return nil end
          emit("ITEM_SEARCH_RESULTS_UPDATED", {itemID=910003}); drain()
          assert(saved.recipe_items[910003].status == "details_unavailable")
          assert(saved.recipes[900004] == nil)
        ''')
        output = ROOT / "artifacts/addon-auction-test-export.lua"
        output.parent.mkdir(exist_ok=True)
        output.write_text("NauctionsProfessionsSaved=" + self.serialize(lua.globals().NauctionsProfessionsSaved), encoding="utf-8")

    def test_full_scan_discovery_is_independent_of_full_scan_recording(self):
        lua = self.auction_client()
        lua.execute('''
          classes[910001] = 9
          tooltips[910001] = {lines={{type=6,spellID=900003}}}
          ADB:Fire("SCAN_DATA", { item={910001,910001}, link=nil }); drain()
          assert(NauctionsProfessionsSaved.recipe_items[910001].status == "resolved")
          assert(ADB.auctionRecipes.attempts[910001] == 1)
        ''')

    def test_valid_use_spell_fallback_and_item_class_fallback(self):
        lua = self.auction_client()
        lua.execute('''
          C_Item.GetItemInfoInstant = nil
          C_Item.GetItemInfo = function(id) return "Localized plan",nil,nil,nil,nil,nil,nil,nil,nil,nil,nil,9,nil,2 end
          C_Item.GetItemSpell = function() return "Craft",900003 end
          ADB:Fire("SCAN_DATA",{item={910001}}); drain()
          assert(NauctionsProfessionsSaved.recipe_items[910001].status == "resolved")
          assert(NauctionsProfessionsSaved.recipes[900003].min_output == 2)
        ''')

    def test_spell_data_events_wake_only_items_waiting_for_that_spell(self):
        lua = self.auction_client()
        lua.execute('''
          classes[910001],classes[910002] = 9,9
          tooltips[910001] = {lines={{type=38,spellID=900003}}}
          tooltips[910002] = {lines={{type=38,spellID=900004}}}
          local original = C_TradeSkillUI.GetRecipeInfo
          local ready = false
          C_TradeSkillUI.GetRecipeInfo = function(id) if ready then return original(id) end end
          ADB:Fire("SCAN_DATA",{item={910001,910002}})
          table.remove(timers,1)() -- First bounded read; both spell details are unavailable.
          assert(ADB.auctionRecipes.waitingSpells[900003][910001])
          assert(ADB.auctionRecipes.waitingSpells[900004][910002])
          ready = true
          emit("SPELL_DATA_LOAD_RESULT",900003)
          assert(ADB.auctionRecipes.queued[910001])
          assert(not ADB.auctionRecipes.queued[910002])
          drain()
          assert(NauctionsProfessionsSaved.recipe_items[910001].status == "resolved")
          assert(ADB.auctionRecipes.waitingSpells[900003] == nil)
        ''')

    def test_missing_metadata_retries_are_bounded_and_resume_on_a_later_observation(self):
        lua = self.auction_client()
        lua.execute('''
          emit("ITEM_SEARCH_RESULTS_UPDATED",{itemID=910001}); drain()
          assert(requests[910001] == 3)
          assert(NauctionsProfessionsSaved.recipe_items[910001] == nil)
          classes[910001] = 9
          tooltips[910001] = {lines={{type=38,spellID=900003}}}
          emit("ITEM_SEARCH_RESULTS_ADDED",{itemID=910001}); drain()
          assert(NauctionsProfessionsSaved.recipe_items[910001].status == "resolved")
        ''')

    def test_ambiguous_and_nonrecipe_wrapper_spells_do_not_create_recipe_facts(self):
        lua = self.auction_client()
        lua.execute('''
          classes[910001],classes[910002] = 9,9
          tooltips[910001] = {lines={{type=38,spellID=900003},{type=38,spellID=900004}}}
          local original = C_TradeSkillUI.GetRecipeSchematic
          C_TradeSkillUI.GetRecipeSchematic = function(id) local s=original(id); if id==999999 then s.recipeID=900005 end; return s end
          ADB:Fire("SCAN_DATA",{item={910001,910002}}); drain()
          assert(NauctionsProfessionsSaved.recipe_items[910001].status == "ambiguous_spell")
          assert(NauctionsProfessionsSaved.recipe_items[910002].status == "details_unavailable")
          assert(next(NauctionsProfessionsSaved.recipes) == nil)
        ''')

    def test_profession_open_retries_unclassified_auction_items_after_metadata_arrives(self):
        lua = self.auction_client()
        lua.execute('''
          emit("ITEM_SEARCH_RESULTS_UPDATED",{itemID=910001}); drain()
          assert(requests[910001] == 3)
          classes[910001] = 9
          tooltips[910001] = {lines={{type=38,spellID=900003}}}
          emit("TRADE_SKILL_SHOW"); drain()
          assert(NauctionsProfessionsSaved.recipe_items[910001].status == "resolved")
        ''')

    def test_pending_recipe_items_resume_after_reload_without_character_state(self):
        lua = self.auction_client()
        lua.execute('''
          classes[910001] = 9
          tooltips[910001] = {lines={{type=38,spellID=900003}}}
          C_TradeSkillUI.GetRecipeInfo = function() return nil end
          emit("ITEM_SEARCH_RESULTS_UPDATED",{itemID=910001}); drain()
        ''')
        saved = self.serialize(lua.globals().NauctionsProfessionsSaved)
        resumed = self.auction_client()
        resumed.execute("NauctionsProfessionsSaved=" + saved)
        resumed.execute('''
          classes[910001] = 9
          tooltips[910001] = {lines={{type=38,spellID=900003}}}
          emit("ADDON_LOADED","AuctionDB"); drain()
          assert(NauctionsProfessionsSaved.recipe_items[910001].status == "resolved")
          assert(NauctionsProfessionsSaved.recipes[900003].learned == nil)
        ''')


if __name__ == "__main__":
    unittest.main()
