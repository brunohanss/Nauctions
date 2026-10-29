import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {ROOT} from './demo-market.mjs';

// Generate the fixtures first, then build strategy-inspect. The Rust model is
// the system under test; this script does not reproduce its detection rules.
const out=path.resolve(process.argv[2]||path.join(ROOT,'artifacts/demo'));
const binary=path.join(ROOT,'target/debug',process.platform==='win32'?'strategy-inspect.exe':'strategy-inspect');
const variants=['irregular','dense','missing_50','missing_80','evening_only','missing_blocks','duplicates','sparse'];
const results={};
for(const variant of variants){
  const run=spawnSync(binary,[path.join(out,`strategy-${variant}.json`)],{encoding:'utf8',maxBuffer:10*1024*1024});
  if(run.error)throw run.error;
  assert.equal(run.status,0,run.stderr);
  const rows=JSON.parse(run.stdout);results[variant]=rows;
  fs.writeFileSync(path.join(out,`results-${variant}.json`),run.stdout);
  for(const row of rows){
    if(row.scenario==='stable'||row.scenario==='falling'||row.scenario==='false_bargain')
      assert.ok(row.patterns.every(p=>!p.detected),`${variant}: false signal for ${row.item_key}`);
    if(row.scenario==='failed_validation')
      assert.ok(row.patterns.filter(p=>p.strategy!=='market_dump').every(p=>p.status!=='watch'),`${variant}: failed validation promoted ${row.item_key}`);
  }
}
assert.deepEqual(results.duplicates,results.dense,'Duplicates changed the analysis');
assert.ok(results.sparse.every(r=>r.patterns.every(p=>p.status==='insufficient')),'Two snapshots must not establish a pattern');
for(const row of results.irregular){
  const expected={weekly_cycle:'weekly_cycle',intraday_recovery:'intraday_recovery',market_dump:'market_dump'}[row.scenario];
  if(!expected)continue;
  const p=row.patterns.find(p=>p.strategy===expected);
  assert.ok(p.detected,`Missing positive control ${row.item_key}: ${expected}`);
  assert.equal(p.status,expected==='market_dump'?'preliminary':'watch');
}
const summary={synthetic:true,anchor:JSON.parse(fs.readFileSync(path.join(out,'scenarios.json'),'utf8')).anchor,variants:{}};
for(const [variant,rows] of Object.entries(results)){
  const patterns=rows.flatMap(r=>r.patterns).filter(p=>p.detected);
  summary.variants[variant]={items:rows.length,detected_patterns:patterns.length,watch:patterns.filter(p=>p.status==='watch').length,preliminary:patterns.filter(p=>p.status==='preliminary').length};
}
fs.writeFileSync(path.join(out,'strategy-validation.json'),JSON.stringify(summary,null,2)+'\n');
console.log(JSON.stringify(summary,null,2));
