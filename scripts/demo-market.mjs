import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import {fileURLToPath} from 'node:url';
export const ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
export const MARKET_ID='de000000-0000-4000-8000-000000000001';
export const items=JSON.parse(fs.readFileSync(path.join(ROOT,'fixtures/demo/items.json'),'utf8'));
const uuid=key=>{const h=crypto.createHash('sha256').update(`nauctions-demo-v1:${key}`).digest('hex');return `${h.slice(0,8)}-${h.slice(8,12)}-4${h.slice(13,16)}-8${h.slice(17,20)}-${h.slice(20,32)}`;};
const quote=s=>`'${String(s).replaceAll("'","''")}'`;
const json=s=>`${quote(JSON.stringify(s))}::jsonb`;
export const unit=(n,d=1)=>({numerator:String(n),denominator:String(d)});
export const value=p=>Number(p.numerator)/Number(p.denominator);
const parisFormatter=new Intl.DateTimeFormat('en-CA',{timeZone:'Europe/Paris',year:'numeric',month:'2-digit',day:'2-digit'});
const localDate=t=>parisFormatter.format(new Date(t));
const lowerMedian=values=>values.sort((a,b)=>a-b)[Math.floor((values.length-1)/2)];
export function reference(dataset,key){
  const today=localDate(dataset.anchor),first=new Date(`${today}T12:00:00Z`);first.setUTCDate(first.getUTCDate()-7);
  const days=new Map();
  for(const scan of dataset.scans){const day=localDate(scan.observed_at);if(day>=first.toISOString().slice(0,10)&&day<today){const values=days.get(day)||[];values.push(value(scan.observations.find(o=>o.item_key===key).median_buyout));days.set(day,values);}}
  return {day_count:days.size,copper:lowerMedian([...days.values()].map(lowerMedian))};
}
export const HISTORY_DAYS=63;
const HOUR=3600000, DAY=24*HOUR;
function randomStream(key){
  let seed=crypto.createHash('sha256').update(key).digest().readUInt32LE(0);
  return ()=>{seed=(seed+0x6D2B79F5)>>>0;let t=seed;t=Math.imul(t^(t>>>15),t|1);t^=t+Math.imul(t^(t>>>7),t|61);return ((t^(t>>>14))>>>0)/4294967296;};
}
export function strategyScenario(item){
  if(item.scenario==='recovery')return 'intraday_recovery';
  if(item.scenario==='stable' && items.indexOf(item)%10===9)return 'failed_validation';
  if(item.scenario==='real_bargain' && items.indexOf(item)%10===7)return 'market_dump';
  return item.scenario;
}
function summarize(listings,item){
  listings.sort((a,b)=>a.buyout_total_copper/a.stack_count-b.buyout_total_copper/b.stack_count);
  const total=listings.reduce((n,l)=>n+l.stack_count,0);let acc=0;
  const middle=listings.find(l=>{acc+=l.stack_count;return acc*2>=total;});
  return {item_key:item.item_key,min_buyout:unit(listings[0].buyout_total_copper,listings[0].stack_count),median_buyout:unit(middle.buyout_total_copper,middle.stack_count),total_units:total,listing_count:listings.length,listings};
}
export function generate(anchor){
  const stamp=new Date(anchor);
  if(!Number.isFinite(stamp.getTime()) || stamp.getUTCFullYear()<2020 || stamp.getUTCFullYear()>2100)throw Error('Use an ISO-8601 anchor between 2020 and 2100.');
  const anchorMs=Math.floor(stamp.getTime()/1000)*1000;
  const schedule=randomStream('early-market-schedule-v3');
  const states=items.map(item=>({random:randomStream(`early-market-v3:${item.item_key}`),walk:0,inventory:0}));
  // Generate the latent market before dropping scans. Scan availability cannot
  // affect prices, supply, or any item's random stream.
  const latent=[];
  for(let hour=0;hour<=HISTORY_DAYS*24;hour++){
    const offset=(HISTORY_DAYS*24-hour)*HOUR;
    const jitter=hour===0||offset===0?0:Math.floor((schedule()-.5)*32)*60000;
    const at=anchorMs-offset+jitter,day=(at-(anchorMs-HISTORY_DAYS*DAY))/DAY;
    const elapsed=(at-anchorMs)/HOUR;
    const phase=((elapsed%24)+24)%24,week=((elapsed%168)+168)%168;
    const observations=items.map((item,i)=>{
      const state=states[i],rand=state.random,role=strategyScenario(item);
      state.walk=state.walk*.88+(rand()-.5)*.035;
      state.inventory=state.inventory*.8+(rand()-.5)*.2;
      const launchNoise=1+.8*Math.exp(-day/10);
      const noise=Math.exp(state.walk*launchNoise);
      let f=1;
      if(item.scenario==='rising')f=.65+1.2*day/HISTORY_DAYS;
      if(item.scenario==='falling')f=2-1.2*day/HISTORY_DAYS;
      if(role==='weekly_cycle')f=week<20||week>164?.85:week<100?1.45:1.08;
      if(role==='intraday_recovery')f=phase<3||phase>22?.80:phase<13?1.40:1.02;
      if(role==='failed_validation')f=day<42?(phase<3||phase>22?.80:phase<13?1.40:1):1;
      if(item.scenario==='supply_shock')f=day>60?1.85:1;
      if(item.scenario==='false_bargain')f=day<30?2.8:1;
      if(role==='market_dump' && offset===0)f=.45;
      const price=Math.max(1,Math.round(item.base_price_copper*f*noise));
      const shortage=item.scenario==='supply_shock'&&day>60;
      const dump=role==='market_dump'&&offset===0;
      const depth=dump?14:shortage?2:3+Math.floor(rand()*4);
      // Thin launch supply grows as players level. Shocks remain item-specific.
      const inventory=Math.max(.15,.32+.5*day/HISTORY_DAYS+state.inventory);
      const listings=Array.from({length:depth},(_,j)=>{
        const stack_count=dump?item.stack_size:Math.max(1,Math.min(item.stack_size,Math.round(item.stack_size*(.25+rand()*.75)*inventory)));
        let spread=.95+j/(depth-1)*.14+(rand()-.5)*.012;
        if(j===0 && offset!==0 && rand()<.02)spread=.75;
        if(item.scenario==='real_bargain' && offset===0 && j===0)spread=.33+(item.id%17)/100;
        const total=Math.max(1,Math.round(price*spread*stack_count));
        return {ordinal:i*20+j,stack_count,buyout_total_copper:total,min_bid_copper:Math.floor(total*.85),current_bid_copper:0,time_left_band:4};
      });
      return summarize(listings,item);
    });
    latent.push({observed_at:new Date(at).toISOString(),observations});
  }
  const selection=randomStream('early-market-player-activity-v3');
  const selected=latent.filter((s,h)=>{
    const ago=HISTORY_DAYS*24-h,daysAgo=Math.floor(ago/24),phase=ago%24;
    // Two multi-day collection outages, plus ordinary missed scans.
    if((daysAgo>=45&&daysAgo<=47)||(daysAgo>=16&&daysAgo<=17))return false;
    const mandatory=phase===0||phase===16||phase===20;
    const probability=h<14*24?.22:phase>=8&&phase<=14?.65:.38;
    const picked=selection()<probability;
    return h===0||ago===0||mandatory||picked;
  });
  // A few short bursts exercise hour balancing without inventing contributors.
  const scans=selected.flatMap((s,i)=>i%31===0&&i>0&&i<selected.length-1?
    [s,{...s,observed_at:new Date(Date.parse(s.observed_at)+3*60000).toISOString()}]:[s])
    .sort((a,b)=>Date.parse(a.observed_at)-Date.parse(b.observed_at))
    .map((s,index)=>({...s,id:uuid(`scan:${index}`),index}));
  const dataset={version:3,synthetic:true,seed:42,history_days:HISTORY_DAYS,anchor:new Date(anchorMs).toISOString(),market:{id:MARKET_ID,region:'EU',realm:'Nauctions Demo',house:'Alliance'},items,scans};
  // Preserve the existing ten bargain examples. This does not change historical
  // strategy prices and does not force any strategy to pass later validation.
  const latest=scans.at(-1);
  latest.observations=latest.observations.map(o=>{
    const item=items.find(i=>i.item_key===o.item_key);
    if(item.scenario==='real_bargain')return o;
    const floor=reference(dataset,item.item_key).copper*.74;
    const listings=o.listings.map(l=>{const total=Math.max(l.buyout_total_copper,Math.ceil(floor*l.stack_count));return {...l,buyout_total_copper:total,min_bid_copper:Math.floor(total*.85)};});
    return summarize(listings,item);
  });
  // Dense and sparse fixtures share the same historical market observations.
  latent.at(-1).observations=latest.observations;
  dataset.latent=latent;
  return dataset;
}
export function strategyDataset(dataset,variant='irregular'){
  const random=randomStream(`early-market-variant:${variant}`);
  const source=variant==='irregular'?dataset.scans:dataset.latent;
  let scans=source.filter(s=>{
    const at=Date.parse(s.observed_at),daysAgo=(Date.parse(dataset.anchor)-at)/DAY;
    if(variant==='sparse')return s===source[0]||s===source.at(-1);
    if(variant==='missing_50')return random()>=.5;
    if(variant==='missing_80')return random()>=.8;
    if(variant==='evening_only'){const h=Number(new Intl.DateTimeFormat('en-GB',{timeZone:'Europe/Paris',hour:'2-digit',hourCycle:'h23'}).format(new Date(at)));return h>=18&&h<24;}
    if(variant==='missing_blocks')return !((daysAgo>=16&&daysAgo<19)||(daysAgo>=43&&daysAgo<46));
    return true;
  });
  if(variant==='duplicates')scans=scans.flatMap(s=>[s,s,s,s,s]);
  return {schema_version:1,synthetic:true,seed:42,as_of:Date.parse(dataset.anchor)/1000,market:dataset.market,variant,
    items:items.map(item=>({item_key:item.item_key,scenario:strategyScenario(item),samples:scans.map(s=>{const o=s.observations.find(o=>o.item_key===item.item_key);return {at:Date.parse(s.observed_at)/1000,price:o.median_buyout,quantity:o.total_units};})}))};
}
export function sql(dataset){
  const lines=['-- SIMULATED ASKING PRICES. No live market evidence. Generated by scripts/demo-market.mjs.','BEGIN;',`INSERT INTO markets(id,region,realm,house) VALUES('${MARKET_ID}','EU','Nauctions Demo','Alliance') ON CONFLICT(region,realm,house) DO NOTHING;`,
    `DO $$ BEGIN IF NOT EXISTS(SELECT 1 FROM markets WHERE id='${MARKET_ID}' AND region='EU' AND realm='Nauctions Demo' AND house='Alliance') THEN RAISE EXCEPTION 'Demo market identity conflict; seed stopped'; END IF; IF EXISTS(SELECT 1 FROM scans WHERE market_id='${MARKET_ID}' AND source_kind<>'demo') THEN RAISE EXCEPTION 'Non-demo scan in demo market; seed stopped'; END IF; END $$;`];
  lines.push(`DO $$ BEGIN IF EXISTS(SELECT 1 FROM scans WHERE id IN (${dataset.scans.map(s=>quote(s.id)).join(',')}) AND (market_id<>'${MARKET_ID}' OR source_kind<>'demo')) THEN RAISE EXCEPTION 'Demo scan identity conflict; seed stopped'; END IF; END $$;`);
  for(const item of items)lines.push(`INSERT INTO items(item_key,base_item_id,name,metadata) VALUES(${quote(item.item_key)},${item.id},${quote(item.name)},${json({class_id:item.class_id,rarity:item.rarity})}) ON CONFLICT(item_key) DO NOTHING;`);
  for(const scan of dataset.scans){
    lines.push(`INSERT INTO scans(id,market_id,fingerprint,source_time,received_at,format_version,quality,source_kind,realm_explicit) VALUES('${scan.id}','${MARKET_ID}',${quote(`nauctions-demo-v1:${scan.index}`)},${quote(scan.observed_at)},${quote(scan.observed_at)},1,'complete','demo',true) ON CONFLICT(id) DO UPDATE SET source_time=excluded.source_time,received_at=excluded.received_at WHERE scans.market_id='${MARKET_ID}' AND scans.source_kind='demo';`);
    // Update only IDs owned by this fixture. A changed anchor reuses the same scan IDs.
    lines.push(`DELETE FROM listings WHERE scan_id='${scan.id}' AND EXISTS(SELECT 1 FROM scans WHERE id='${scan.id}' AND market_id='${MARKET_ID}' AND source_kind='demo');`);
    lines.push(`DELETE FROM item_observations WHERE scan_id='${scan.id}' AND EXISTS(SELECT 1 FROM scans WHERE id='${scan.id}' AND market_id='${MARKET_ID}' AND source_kind='demo');`);
    lines.push(`INSERT INTO item_observations(scan_id,item_key,min_buyout,median_buyout,total_units,listing_count) VALUES\n`+scan.observations.map(o=>`('${scan.id}',${quote(o.item_key)},${json(o.min_buyout)},${json(o.median_buyout)},${o.total_units},${o.listing_count})`).join(',\n')+` ON CONFLICT(scan_id,item_key) DO UPDATE SET min_buyout=excluded.min_buyout,median_buyout=excluded.median_buyout,total_units=excluded.total_units,listing_count=excluded.listing_count;`);
    lines.push(`INSERT INTO listings(scan_id,item_key,ordinal,stack_count,buyout_total_copper,min_bid_copper,current_bid_copper,time_left_band) VALUES\n`+scan.observations.flatMap(o=>o.listings.map(l=>`('${scan.id}',${quote(o.item_key)},${l.ordinal},${l.stack_count},${l.buyout_total_copper},${l.min_bid_copper},0,4)`)).join(',\n')+';');
  }
  lines.push('COMMIT;');return lines.join('\n')+'\n';
}
export function manifest(dataset){
  const latest=dataset.scans.at(-1);
  return {version:3,synthetic:true,seed:42,history_days:HISTORY_DAYS,anchor:dataset.anchor,market:dataset.market,scan_count:dataset.scans.length,observation_count:dataset.scans.length*items.length,listing_count:dataset.scans.reduce((n,s)=>n+s.observations.reduce((n,o)=>n+o.listing_count,0),0),scenarios:items.map(item=>{const latest_min_copper=value(latest.observations.find(o=>o.item_key===item.item_key).min_buyout),baseline=reference(dataset,item.item_key);return {item_key:item.item_key,name:item.name,scenario:item.scenario,strategy_scenario:strategyScenario(item),latest_min_copper,reference_7d_copper:baseline.copper,reference_day_count:baseline.day_count,discount_percentage:100*(1-latest_min_copper/baseline.copper),expected:item.scenario==='real_bargain'?'qualifies_under_7d_reference_30_percent':item.scenario==='false_bargain'?'does_not_qualify_despite_30d_spike':`visible_${item.scenario}`};})};
}
if(process.argv[1] && path.resolve(process.argv[1])===fileURLToPath(import.meta.url)){
  const args=process.argv.slice(2);const get=(k,fallback)=>args.includes(k)?args[args.indexOf(k)+1]:fallback;
  if(args.some(a=>a.startsWith('--')&&!['--anchor','--out'].includes(a)))throw Error('Usage: node scripts/demo-market.mjs [--anchor ISO8601] [--out directory]');
  const dataset=generate(get('--anchor',new Date().toISOString()));
  const out=path.resolve(get('--out',path.join(ROOT,'artifacts/demo')));fs.mkdirSync(out,{recursive:true});
  fs.writeFileSync(path.join(out,'seed.sql'),sql(dataset));
  fs.writeFileSync(path.join(out,'scenarios.json'),JSON.stringify(manifest(dataset),null,2)+'\n');
  for(const variant of ['irregular','dense','missing_50','missing_80','evening_only','missing_blocks','duplicates','sparse'])fs.writeFileSync(path.join(out,`strategy-${variant}.json`),JSON.stringify(strategyDataset(dataset,variant))+'\n');
  fs.writeFileSync(path.join(out,'README.md'),`# Synthetic early-market fixture\n\nGenerated by scripts/demo-market.mjs. All prices and supply are invented.\nReplay: ${dataset.anchor}. Fixed seed label: 42; version: 3.\n${dataset.history_days} days, ${dataset.scans.length} irregular scans, 50 Classic items.\n\nseed.sql populates only the Nauctions Demo market. strategy-*.json are analysis fixtures.\nDense and sparse variants share one latent market. Missing scans stay unknown.\nThe sampling schedule includes selected recurring sessions so positive controls remain testable.\nThis is a controlled simulation, not an estimate of real player activity.\nRun scripts/verify-demo-strategies.mjs to measure outcomes with the Rust model.\nSee docs/demo-market.md for commands and scenario definitions.\n`);
  console.log(JSON.stringify({output:out,...manifest(dataset),scenarios:undefined},null,2));
}

