import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {generate,manifest,reference,sql,value,items,MARKET_ID,strategyDataset,strategyScenario,HISTORY_DAYS} from './demo-market.mjs';
const data=generate('2026-10-27T16:00:00Z');
const series=item=>data.scans.map(s=>s.observations.find(o=>o.item_key===item.item_key));
test('50 unique Classic identities, bounded observations and deterministic anchored seed',()=>{
 assert.equal(items.length,50);assert.equal(new Set(items.map(i=>i.id)).size,50);
 const category=id=>id===0?'consumables':id===7?'materials':id===9?'recipes':[2,4].includes(id)?'equipment':'other';
 for(const group of ['materials','consumables','recipes','equipment','other']){const members=items.filter(item=>category(item.class_id)===group);assert.equal(members.length,10);assert.equal(members.filter(item=>item.scenario==='real_bargain').length,2);}
 assert.equal(data.scans.length,701);assert.ok(data.scans.length*50<50000);
 assert.equal(data.scans.at(-1).observed_at,data.anchor);
 assert.equal(Date.parse(data.anchor)-Date.parse(data.scans[0].observed_at),HISTORY_DAYS*86400000);
 assert.deepEqual(generate(data.anchor),data);
 const shifted=generate('2026-10-28T16:00:00Z');assert.deepEqual(shifted.scans.map(s=>s.id),data.scans.map(s=>s.id));
 assert.throws(()=>generate('invalid'));
 assert.deepEqual(manifest(data),JSON.parse(fs.readFileSync(new URL('../fixtures/demo/scenarios.json',import.meta.url),'utf8')));
});
test('every observation agrees with actual listing depth and weighted unit-price median',()=>{
 for(const scan of data.scans){assert.equal(scan.observations.length,50);assert.equal(new Set(scan.observations.flatMap(o=>o.listings.map(l=>l.ordinal))).size,scan.observations.reduce((n,o)=>n+o.listings.length,0));
 for(const o of scan.observations){const ls=o.listings,total=ls.reduce((n,l)=>n+l.stack_count,0);let acc=0;const mid=ls.find(l=>{acc+=l.stack_count;return acc*2>=total;});assert.equal(total,o.total_units);assert.equal(o.listing_count,ls.length);assert.equal(value(o.min_buyout),ls[0].buyout_total_copper/ls[0].stack_count);assert.equal(value(o.median_buyout),mid.buyout_total_copper/mid.stack_count);for(const l of ls){assert.ok(l.stack_count>0);assert.ok(l.buyout_total_copper>0);assert.ok(l.min_bid_copper<=l.buyout_total_copper);}}
 }
});
test('irregular scans include bursts and outages without changing the latent market',()=>{
 const times=data.scans.map(s=>Date.parse(s.observed_at));
 const gaps=times.slice(1).map((t,i)=>t-times[i]);
 assert.ok(Math.max(...gaps)>2*86400000);
 assert.ok(gaps.includes(3*60000));assert.ok(new Set(gaps).size>30);
 const dense=strategyDataset(data,'dense'),irregular=strategyDataset(data);
 assert.equal(dense.items[0].samples.length,HISTORY_DAYS*24+1);
 for(const item of irregular.items){
   const latent=new Map(dense.items.find(i=>i.item_key===item.item_key).samples.map(s=>[s.at,s]));
   // Burst observations repeat a nearby underlying snapshot; all others are exact subsets.
   for(const sample of item.samples)if(latent.has(sample.at))assert.deepEqual(sample,latent.get(sample.at));
 }
 const sparse=strategyDataset(data,'sparse');assert.equal(sparse.items[0].samples.length,2);
 const duplicates=strategyDataset(data,'duplicates');assert.equal(duplicates.items[0].samples.length,dense.items[0].samples.length*5);
 for(const item of items){const points=series(item),prices=points.map(p=>value(p.median_buyout)),first=prices[0],last=prices.at(-1);
   if(strategyScenario(item)==='stable')assert.ok(Math.max(...prices)/Math.min(...prices)<1.3);
   if(item.scenario==='rising')assert.ok(last>first*2);
   if(item.scenario==='falling')assert.ok(last<first*.6);
   if(item.scenario==='supply_shock')assert.ok(last>first*1.5);
   assert.ok(new Set(points.map(p=>p.total_units)).size>2);
 }
});

test('all true bargains clear 30 percent, all false bargains fail robust recent comparison, including DST',()=>{
 for(const anchor of ['2026-10-27T16:00:00Z','2026-03-30T00:05:00Z','2026-10-25T00:05:00Z']){const d=generate(anchor);for(const row of manifest(d).scenarios){assert.equal(row.reference_day_count,7);if(row.scenario==='real_bargain')assert.ok(row.discount_percentage>40);if(row.scenario!=='real_bargain')assert.ok(row.discount_percentage<30,JSON.stringify(row));}}
});
test('SQL is transactional, scoped, idempotent and preserves shared catalog data',()=>{
 const s=sql(data);assert.ok(s.includes('BEGIN;'));assert.ok(s.endsWith('COMMIT;\n'));assert.ok(s.includes("'complete','demo',true"));assert.ok(s.includes('Demo scan identity conflict'));assert.ok(s.includes(`market_id='${MARKET_ID}' AND source_kind='demo'`));assert.ok(s.includes('ON CONFLICT(item_key) DO NOTHING'));assert.ok(!/DELETE FROM (markets|items|scans)/.test(s));assert.ok(!s.includes('TRUNCATE'));
 const deletes=s.split('\n').filter(line=>line.startsWith('DELETE FROM '));assert.equal(deletes.length,data.scans.length*2);for(const line of deletes)assert.match(line,new RegExp(`WHERE scan_id='[a-f0-9-]+' AND EXISTS\\(SELECT 1 FROM scans WHERE id='[a-f0-9-]+' AND market_id='${MARKET_ID}' AND source_kind='demo'\\);$`));
});
