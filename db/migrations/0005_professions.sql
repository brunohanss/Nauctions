-- Shared facts only. This schema has no character or learned-recipe tables.
CREATE TABLE professions (
 product text NOT NULL, skill_line_id bigint NOT NULL CHECK(skill_line_id>0),
 name text NOT NULL, PRIMARY KEY(product,skill_line_id)
);
CREATE TABLE recipe_versions (
 id text PRIMARY KEY, product text NOT NULL, build text NOT NULL,
 build_number bigint NOT NULL CHECK(build_number>=69876),
 spell_id bigint NOT NULL CHECK(spell_id>0), profession_id bigint NOT NULL,
 name text NOT NULL, locale text NOT NULL,
 kind text NOT NULL CHECK(kind IN ('item','service')),
 facts jsonb NOT NULL, first_observed_at timestamptz NOT NULL,
 last_observed_at timestamptz NOT NULL,
 FOREIGN KEY(product,profession_id) REFERENCES professions(product,skill_line_id)
);
CREATE INDEX recipe_versions_lookup ON recipe_versions(product,spell_id,build_number DESC,last_observed_at DESC);
CREATE INDEX recipe_versions_profession ON recipe_versions(product,profession_id);
CREATE TABLE recipe_reagents (
 recipe_id text NOT NULL REFERENCES recipe_versions ON DELETE CASCADE,
 item_id bigint NOT NULL CHECK(item_id>0), quantity bigint NOT NULL CHECK(quantity>0),
 PRIMARY KEY(recipe_id,item_id)
);
CREATE TABLE recipe_outputs (
 recipe_id text PRIMARY KEY REFERENCES recipe_versions ON DELETE CASCADE,
 item_id bigint NOT NULL CHECK(item_id>0),
 min_quantity bigint CHECK(min_quantity>0), max_quantity bigint CHECK(max_quantity>0),
 CHECK((min_quantity IS NULL) = (max_quantity IS NULL)),
 CHECK(max_quantity>=min_quantity)
);
CREATE INDEX recipe_outputs_item ON recipe_outputs(item_id);
CREATE TABLE recipe_observations (
 recipe_id text NOT NULL REFERENCES recipe_versions,
 upload_id uuid NOT NULL REFERENCES uploads,
 observed_at timestamptz NOT NULL, addon_version text NOT NULL, locale text NOT NULL,
 facts jsonb NOT NULL, PRIMARY KEY(recipe_id,upload_id)
);
