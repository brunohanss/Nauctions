import fs from 'node:fs';
const rows=JSON.parse(fs.readFileSync(new URL('../fixtures/demo/catalog-rows.json',import.meta.url)));
const scenarios=['real_bargain','stable','rising','falling','weekly_cycle','recovery','supply_shock','real_bargain','false_bargain','stable'];
const category=id=>id===0?'consumables':id===7?'materials':id===9?'recipes':[2,4].includes(id)?'equipment':'other';
const items=rows.map(([id,name,class_id,rarity,base_price_copper,stack_size],i)=>({id,item_key:`i${id}`,name,class_id,rarity,base_price_copper,stack_size,scenario:scenarios[i%10]}));
if(items.length!==50||new Set(items.map(item=>item.id)).size!==50)throw new Error('The demo must contain 50 unique items.');
for(const group of ['materials','consumables','recipes','equipment','other']){
  const members=items.filter(item=>category(item.class_id)===group);
  if(members.length!==10||members.filter(item=>item.scenario==='real_bargain').length!==2)throw new Error(`Expected 10 items and 2 real bargains in ${group}.`);
}
fs.writeFileSync(new URL('../fixtures/demo/items.json',import.meta.url),JSON.stringify(items,null,2)+'\n');
