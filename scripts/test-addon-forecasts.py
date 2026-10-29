"""Run only trusted addon modules and synthetic cache fixtures in Lua 5.1."""
from pathlib import Path
import json
import math
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'artifacts/lua-test-runtime'))
from lupa.lua51 import LuaRuntime


def client():
    lua = LuaRuntime(unpack_returned_tuples=True)
    lua.execute('''
      ADB={db={tooltip=true},sv={},commands={}}
      function ADB:AddCommand(n,f) self.commands[n]=f end
      function ADB:Print() end
      function ADB:PlayerHouse() return 'Horde' end
      function ADB:Money(n) return tostring(n) end
      function GetRealmName() return 'Normal Realm' end
      function GetServerTime() return 100000 end
      function IsAltKeyDown() return true end
      Enum={TooltipDataType={Item=0}}
      TooltipDataProcessor={AddTooltipPostCall=function() end}
    ''')
    for file in ['NauctionsBeta.lua', 'NauctionsForecasts.lua']:
        lua.execute((ROOT / 'addon/AuctionDB' / file).read_text(encoding='utf8'), 'AuctionDB', lua.globals().ADB)
    return lua


class ForecastTests(unittest.TestCase):
    @unittest.skipUnless((ROOT / 'artifacts/forecast-cache/ForecastData.lua').exists(), 'Run PostgreSQL forecast integration tests first')
    def test_actual_server_cache_is_readable_by_the_addon(self):
        lua = client()
        lua.execute((ROOT / 'artifacts/forecast-cache/ForecastData.lua').read_text(encoding='utf8'))
        lua.execute('''
          function GetRealmName() return 'Synthetic Forecast Realm' end
          local cache=NauctionsForecastCache
          local row=ADB:ForecastRecord('i1180',cache.generated_at+1)
          assert(row and row.rolling.status=='baseline')
          assert(row.rolling.price.numerator=='1000')
          assert(row.scenario[1].status=='scenario')
          assert(row.reference.beta_reset_time_known==false)
          assert(not ADB:ForecastRecord('i1180',cache.expires_at))
        ''')
    def test_bundled_app_and_addon_data_match_every_item_and_day(self):
        lua = client()
        source = json.loads((ROOT / 'assets/beta-normal-horde.json').read_text(encoding='utf8'))
        beta = lua.globals().ADB.betaReference
        self.assertEqual(sum(1 for _ in beta['items'].items()), 3458)
        count = 0
        for item in source['items']:
            actual = beta['items'][item['id']]
            self.assertEqual(actual.name, item['name'])
            self.assertEqual(actual.variants_combined, item['variants_combined'])
            self.assertEqual(len(actual.points), len(item['points']))
            for i, point in enumerate(item['points'], 1):
                self.assertEqual([actual.points[i][n] for n in range(1, 5)], [False if v is None else v for v in point])
                count += 1
        self.assertEqual(count, 11298)
        self.assertEqual(list(lua.globals().ADB.sv.items()), [])

    def test_scenarios_do_not_extrapolate_or_invent_reset_dates(self):
        lua = client()
        adb = lua.globals().ADB
        self.assertAlmostEqual(adb.BetaScenario(adb, 1180, 0, 1), 0, places=9)
        expected = (math.exp((math.log(199) - math.log(99)) * 6 / 7) - 1) * 100
        self.assertAlmostEqual(adb.BetaScenario(adb, 1180, 0, 7), expected, places=9)
        self.assertIsNone(adb.BetaScenario(adb, 1180, 17, 1))
        self.assertIsNone(adb.BetaScenario(adb, 1180, -1, 1))
        self.assertIsNone(adb.BetaScenario(adb, 99999999, 0, 1))

    def test_cache_rejects_expiry_wrong_market_future_data_and_variant_fallback(self):
        lua = client()
        lua.execute('''
          NauctionsForecastCache={schema_version=1,realm='Normal Realm',house='Horde',generated_at=100000,expires_at=100100,records={
            {schema_version=1,item_key='i1180',market_id='fixture',realm='Normal Realm',house='Horde',generated_at=100000,expires_at=100100,
             rolling={status='baseline',price={numerator='100',denominator='1'},low={numerator='90',denominator='1'},high={numerator='110',denominator='1'}}}}}
          assert(ADB:ForecastRecord('i1180',100000))
          assert(not ADB:ForecastRecord('i1180',100100))
          assert(not ADB:ForecastRecord('i1180:1',100000))
          NauctionsForecastCache.realm='Other';assert(not ADB:ForecastRecord('i1180'));NauctionsForecastCache.realm='Normal Realm'
          NauctionsForecastCache.house='Alliance';assert(not ADB:ForecastRecord('i1180'));NauctionsForecastCache.house='Horde'
          NauctionsForecastCache.generated_at=100301;assert(not ADB:ForecastRecord('i1180'));NauctionsForecastCache.generated_at=100000
          NauctionsForecastCache.records[1].demo=true;assert(not ADB:ForecastRecord('i1180'))
        ''')

    def test_tooltips_keep_beta_and_forecasts_separate_from_observed_history(self):
        lua = client()
        lua.execute('''
          local tt={lines={}}
          function tt:GetItem() return 'Scroll','|cffffffff|Hitem:1180:0:0:0:0:0:0:0:0:0:0:0|h[Scroll]|h|r' end
          function tt:AddLine(s) table.insert(self.lines,s) end
          function tt:AddDoubleLine(s,v) table.insert(self.lines,s..': '..v) end
          ADB:ForecastTooltip(tt,{id=1180,type=0})
          local lines=table.concat(tt.lines,'\\n')
          assert(lines:find('Beta Normal Horde median',1,true))
          assert(lines:find('not reset age',1,true))
          assert(lines:find('Data from ForeverDB.net',1,true))
          assert(not ADB.sv.houses and not ADB.sv.ah)
          ADB.commands.forecastday(ADB,'8');assert(ADB.sv.nauctionsForecastDay==8)
          ADB.commands.forecastday(ADB,'99');assert(ADB.sv.nauctionsForecastDay==8)
        ''')


if __name__ == '__main__':
    unittest.main()
