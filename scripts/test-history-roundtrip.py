"""Run trusted synthetic Rust output through bundled addon initialization/history.

This is a Lua simulation. It does not establish target-game compatibility.
Run `cargo run -p ahdb-format --example offline-sync --locked` first.
Never substitute user SavedVariables for these generated fixtures.
"""
from pathlib import Path
import hashlib
import json
import sys
import unittest

import importlib.util

ROOT = Path(__file__).resolve().parent.parent
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("addon_test_support", ROOT / "scripts/test-addon.py")
support = importlib.util.module_from_spec(spec)
spec.loader.exec_module(support)
LuaRuntime = support.LuaRuntime


class HistoryRoundtrip(unittest.TestCase):
    def client(self, saved):
        lua = LuaRuntime(unpack_returned_tuples=True)
        lua.execute(r'''
            now = 1793101000
            date = os.date
            SlashCmdList = {}
            C_AddOns = {GetAddOnMetadata = function() return "synthetic-test" end}
            function GetServerTime() return now end
            function GetTime() return 1 end
            function GetRealmName() return "Synthetic Realm" end
            function GetUnitName() return "SYNTHETIC" end
            function GetLocale() return "enUS" end
            function UnitFactionGroup() return "Alliance" end
            function wipe(t) for k in pairs(t) do t[k] = nil end end
            function CreateFrame()
                frame = {RegisterEvent = function() end,
                         SetScript = function(_, _, fn) event = fn end}
                return frame
            end
            C_Item = {GetItemInfo = function(link)
                return "Synthetic", link, 1, 1, 1, nil, nil, 20, nil, nil, 1, 7, 0
            end}
            ADB = {}
        ''')
        # Only generated synthetic output and checked-in addon code are executed.
        lua.execute(saved)
        for name in ["AuctionDB.lua", "History.lua", "Record.lua"]:
            lua.execute((ROOT / "addon/AuctionDB" / name).read_text(encoding="utf-8"), "AuctionDB", lua.globals().ADB)
        lua.execute('event(nil, "ADDON_LOADED", "AuctionDB"); event(nil, "PLAYER_LOGIN")')
        return lua

    def test_two_synthetic_players_load_compact_append_and_reload(self):
        evidence = {"kind": "synthetic_lua_simulation", "real_client_validated": False, "players": []}
        directory = ROOT / "artifacts/offline-sync"
        self.assertTrue(directory.is_dir(), "Generate fixtures with the Rust offline-sync example first")
        for player in ["a", "b"]:
            with self.subTest(player=player):
                merged = (directory / f"player-{player}-merged.lua").read_text(encoding="utf-8")
                lua = self.client(merged)
                lua.execute('''
                    local h = ADB:House("Alliance")
                    assert(#ADB:RecentHistory(789, "Alliance") == 1)
                    assert(#ADB:DailyHistory(789, "Alliance") == 1)
                    assert(#ADB:StoredDaily(789, "Alliance") == 0)
                    assert(ADB:LastPrice(789, "Alliance")[2] == 4500)
                    local low, median, days = ADB:Typical(789, "Alliance")
                    assert(low == 4500 and median == 4600 and days == 1)
                    now = now + 8 * 86400
                    ADB:CompactHistory(h, now)
                    assert(#ADB:RecentHistory(789, "Alliance") == 0)
                    assert(ADB:StoredDaily(789, "Alliance")[1][5] == 1)
                    ADB:AddHistoryPoint(h, 790, now, 500, 500, 2)
                    h.scans[#h.scans + 1] = {now, 1, 1}
                    ADB.scanHouseName = "Alliance"; ADB.scanStart = 0
                    ADB:RecordScan({n=1, link={"|cffffffff|Hitem:790:::::::::::::|h[Synthetic new item]|h|r"},
                        allTimeLeft={1}, count={2}, minBid={0}, buyout={1000}, curBid={0}})
                    assert(ADB:LastPrice(790, "Alliance")[2] == 500)
                    assert(ADB.sv.nauctionsSync.realm == "Synthetic Realm")
                ''')
                saved = "\n".join(name + "=" + support.AddonTests.serialize(lua.globals()[name])
                    for name in ["AuctionDBForeverSaved", "NauctionsProfessionsSaved"])
                resumed = self.client(saved)
                resumed.execute('''
                    assert(ADB.sv.nauctionsSync.house == "Alliance")
                    assert(ADB:LastPrice(789, "Alliance")[2] == 4500)
                    assert(#ADB.sv.ah == 2)
                    assert(NauctionsProfessionsSaved.recipes[900001].reagents[900201] == 3)
                ''')
                (directory / f"player-{player}-reloaded.lua").write_text(saved, encoding="utf-8")
                evidence["players"].append({"id": player, "sha256": hashlib.sha256(saved.encode()).hexdigest()})
        self.assertEqual(len(evidence["players"]), 2)
        evidence["addon_sha256"] = {name: hashlib.sha256((ROOT / "addon/AuctionDB" / name).read_bytes()).hexdigest()
                                    for name in ["AuctionDB.lua", "History.lua", "Record.lua"]}
        (directory / "evidence.json").write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    unittest.main()
