-- Local UI examples only. Recipe names, ingredients, yields, and spell IDs are invented.
-- Do not load this file into a shared or production database.
BEGIN;
DO $$ BEGIN
  IF NOT EXISTS (SELECT 1 FROM markets m JOIN scans s ON s.market_id=m.id
    WHERE m.realm='Nauctions Demo' AND s.source_kind='demo') THEN
    RAISE EXCEPTION 'Load the local demo market first';
  END IF;
END $$;

-- Invented merchant buyback prices exercise profitable and loss-making sales.
-- These item metadata changes are confined to the local development database.
UPDATE items SET metadata=metadata || jsonb_build_object('sell_price',example.copper)
FROM (VALUES (4238,4000),(14046,2500),(13444,2500),(13446,1000)) AS example(item_id,copper)
WHERE items.base_item_id=example.item_id;

INSERT INTO professions(product,skill_line_id,name) VALUES
  ('wow_classic_beta',171,'Alchemy'),
  ('wow_classic_beta',164,'Blacksmithing'),
  ('wow_classic_beta',197,'Tailoring'),
  ('wow_classic_beta',185,'Cooking'),
  ('wow_classic_beta',333,'Enchanting')
ON CONFLICT DO NOTHING;

WITH examples(spell_id,profession_id,profession_name,name,kind,output,min_output,max_output,tradable,reagents) AS (VALUES
  (990000001,171,'Alchemy','Demo: Mana potion','item',13444,1,1,true,'{"13463":2}'::jsonb),
  (990000002,171,'Alchemy','Demo: Healing potion','item',13446,1,1,true,'{"13463":1}'::jsonb),
  (990000003,164,'Blacksmithing','Demo: Copper mace','item',2844,1,1,true,'{"2770":3}'::jsonb),
  (990000004,164,'Blacksmithing','Demo: Expensive copper axe','item',2845,1,1,true,'{"12360":2}'::jsonb),
  (990000005,197,'Tailoring','Demo: Linen bag','item',4238,1,1,true,'{"2996":3}'::jsonb),
  (990000006,197,'Tailoring','Demo: Runecloth bag','item',14046,1,1,true,'{"14047":4}'::jsonb),
  (990000007,185,'Cooking','Demo: Variable soup yield','item',13931,1,3,true,'{"13463":1}'::jsonb),
  (990000008,333,'Enchanting','Demo: Enchant service','service',NULL::int,NULL::int,NULL::int,false,'{"14344":2}'::jsonb),
  (990000009,171,'Alchemy','Demo: Missing reagent price','item',5634,1,1,true,'{"999999999":1}'::jsonb),
  (990000010,171,'Alchemy','Demo: Missing output price','item',999999998,1,1,true,'{"13463":1}'::jsonb),
  (990000011,197,'Tailoring','Demo: Bolt of linen cloth','item',2996,1,1,true,'{"2589":2}'::jsonb)
), anchor AS (
  SELECT max(source_time) AS at FROM scans s JOIN markets m ON m.id=s.market_id
  WHERE m.realm='Nauctions Demo' AND s.source_kind='demo'
)
INSERT INTO recipe_versions(id,product,build,build_number,spell_id,profession_id,name,locale,kind,facts,first_observed_at,last_observed_at)
SELECT 'nauctions-demo-crafting:'||spell_id,'wow_classic_beta','1.60.1.70685-demo',70685,
  spell_id,profession_id,name,'enUS',kind,
  jsonb_build_object('spell_id',spell_id,'profession_id',profession_id,'profession_name',profession_name,
    'name',name,'captured_at',extract(epoch from at)::bigint,'kind',kind,'output_item_id',output,
    'min_output',min_output,'max_output',max_output,'tradable',tradable,'reagents',reagents,
    'tools','[]'::jsonb,'cooldown_seconds',NULL),at,at
FROM examples CROSS JOIN anchor
ON CONFLICT(id) DO UPDATE SET facts=excluded.facts,last_observed_at=excluded.last_observed_at;

-- Remove obsolete example ingredients when this fixture is loaded again.
DELETE FROM recipe_reagents r USING recipe_versions v
WHERE r.recipe_id=v.id AND v.id LIKE 'nauctions-demo-crafting:%'
  AND NOT (v.facts->'reagents' ? r.item_id::text);

INSERT INTO recipe_reagents(recipe_id,item_id,quantity)
SELECT id,r.key::bigint,r.value::bigint FROM recipe_versions
CROSS JOIN LATERAL jsonb_each_text(facts->'reagents') r
WHERE id LIKE 'nauctions-demo-crafting:%'
ON CONFLICT(recipe_id,item_id) DO UPDATE SET quantity=excluded.quantity;

INSERT INTO recipe_outputs(recipe_id,item_id,min_quantity,max_quantity)
SELECT id,(facts->>'output_item_id')::bigint,(facts->>'min_output')::bigint,(facts->>'max_output')::bigint
FROM recipe_versions WHERE id LIKE 'nauctions-demo-crafting:%' AND kind='item'
ON CONFLICT(recipe_id) DO UPDATE SET item_id=excluded.item_id,min_quantity=excluded.min_quantity,max_quantity=excluded.max_quantity;
COMMIT;
