-- Immutable auction recipe-item discoveries. Missing recipe details are explicit.
CREATE TABLE recipe_item_observations (
 product text NOT NULL, build text NOT NULL, build_number bigint NOT NULL CHECK(build_number>=69876),
 item_id bigint NOT NULL CHECK(item_id>0), upload_id uuid NOT NULL REFERENCES uploads,
 observed_at timestamptz NOT NULL, recipe_spell_id bigint CHECK(recipe_spell_id>0),
 status text NOT NULL CHECK(status IN ('resolved','mapping_unavailable','details_unavailable','ambiguous_spell')),
 CHECK(status<>'resolved' OR recipe_spell_id IS NOT NULL),
 PRIMARY KEY(product,build,item_id,upload_id)
);
CREATE INDEX recipe_item_observations_latest ON recipe_item_observations(product,item_id,build_number DESC,observed_at DESC);
