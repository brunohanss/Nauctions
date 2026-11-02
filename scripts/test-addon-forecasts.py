"""Run only trusted addon modules and synthetic cache fixtures in Lua 5.1."""
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'artifacts/lua-test-runtime'))
from lupa.lua51 import LuaRuntime


def client():
    lua = LuaRuntime(unpack_returned_tuples=True)
    lua.execute('''
      ADB={db={tooltip=true},sv={}}
      function ADB:PlayerHouse() return 'Horde' end
      function ADB:Money(n) return tostring(n) end
      function GetRealmName() return 'Normal Realm' end
      function GetServerTime() return 100000 end
      Enum={TooltipDataType={Item=0}}
      TooltipDataProcessor={AddTooltipPostCall=function() end}
    ''')
    lua.execute((ROOT / 'addon/AuctionDB/NauctionsForecasts.lua').read_text(encoding='utf8'), 'AuctionDB', lua.globals().ADB)
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
          function GetServerTime() return cache.generated_at+1 end
          local tt={lines={}}
          function tt:GetItem() return 'Scroll','|cffffffff|Hitem:1180:0:0:0:0:0:0:0:0:0:0:0|h[Scroll]|h|r' end
          function tt:AddLine(s) table.insert(self.lines,s) end
          function tt:AddDoubleLine(s,v) table.insert(self.lines,s..': '..v) end
          ADB:ForecastTooltip(tt,{id=1180,type=0})
          local lines=table.concat(tt.lines,'\\n')
          assert(lines:find('24h median (unchanged baseline): 1000',1,true))
          assert(lines:find('24h historical-error range',1,true))
          assert(not ADB.sv.houses and not ADB.sv.ah)
          assert(not ADB:ForecastRecord('i1180',cache.expires_at))
        ''')
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

    def test_tooltips_explain_missing_cache_without_changing_observed_history(self):
        lua = client()
        lua.execute('''
          local tt={lines={}}
          function tt:GetItem() return 'Scroll','|cffffffff|Hitem:1180:0:0:0:0:0:0:0:0:0:0:0|h[Scroll]|h|r' end
          function tt:AddLine(s) table.insert(self.lines,s) end
          function tt:AddDoubleLine(s,v) table.insert(self.lines,s..': '..v) end
          ADB:ForecastTooltip(tt,{id=1180,type=0})
          local lines=table.concat(tt.lines,'\\n')
          assert(lines:find('24h forecast:',1,true))
          assert(not ADB.sv.houses and not ADB.sv.ah)
        ''')


if __name__ == '__main__':
    unittest.main()
