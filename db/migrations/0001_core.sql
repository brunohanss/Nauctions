CREATE TABLE members (
 id uuid PRIMARY KEY, login text UNIQUE NOT NULL, display_name text NOT NULL,
 password_hash text NOT NULL, role text NOT NULL CHECK(role IN ('admin','member')),
 status text NOT NULL DEFAULT 'active' CHECK(status IN ('active','disabled')), created_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE sessions (token_hash text PRIMARY KEY, member_id uuid NOT NULL REFERENCES members, csrf_token text NOT NULL, expires_at timestamptz NOT NULL);
CREATE TABLE invitations (code_hash text PRIMARY KEY, role text NOT NULL CHECK(role IN ('admin','member')), expires_at timestamptz NOT NULL, used_at timestamptz);
CREATE TABLE pairing_codes (code_hash text PRIMARY KEY, member_id uuid NOT NULL REFERENCES members, expires_at timestamptz NOT NULL, used_at timestamptz);
CREATE TABLE devices (id uuid PRIMARY KEY, member_id uuid NOT NULL REFERENCES members, display_name text NOT NULL, credential_hash text UNIQUE NOT NULL, created_at timestamptz NOT NULL DEFAULT now(), last_seen_at timestamptz, revoked_at timestamptz, health jsonb);
CREATE TABLE markets (id uuid PRIMARY KEY, region text NOT NULL DEFAULT 'EU', realm text NOT NULL, house text NOT NULL CHECK(house IN ('Alliance','Horde','Neutral')), UNIQUE(region,realm,house));
CREATE TABLE source_files (device_id uuid NOT NULL REFERENCES devices, source_file_id text NOT NULL, last_seen_at timestamptz NOT NULL DEFAULT now(), last_hash text NOT NULL, PRIMARY KEY(device_id,source_file_id));
CREATE TABLE uploads (id uuid PRIMARY KEY, device_id uuid NOT NULL REFERENCES devices, upload_id uuid NOT NULL, checksum text NOT NULL, byte_count bigint NOT NULL CHECK(byte_count>0), source_file_id text NOT NULL, status text NOT NULL DEFAULT 'received' CHECK(status IN ('received','accepted','rejected')), received_at timestamptz NOT NULL DEFAULT now(), error_code text, staged_bytes bytea, UNIQUE(device_id,upload_id));
CREATE TABLE scans (id uuid PRIMARY KEY, market_id uuid NOT NULL REFERENCES markets, fingerprint text NOT NULL, source_time timestamptz NOT NULL, received_at timestamptz NOT NULL DEFAULT now(), format_version int NOT NULL, quality text NOT NULL CHECK(quality IN ('unknown','partial','complete','history')), source_kind text NOT NULL, UNIQUE(market_id,fingerprint));
CREATE INDEX scans_market_time ON scans(market_id,source_time DESC);
CREATE TABLE scan_sources (scan_id uuid NOT NULL REFERENCES scans, device_id uuid NOT NULL REFERENCES devices, upload_id uuid NOT NULL REFERENCES uploads, PRIMARY KEY(scan_id,device_id,upload_id));
CREATE TABLE items (item_key text PRIMARY KEY, base_item_id bigint NOT NULL CHECK(base_item_id>0), name text NOT NULL, metadata jsonb NOT NULL DEFAULT '{}');
CREATE TABLE listings (scan_id uuid NOT NULL REFERENCES scans, item_key text NOT NULL REFERENCES items, ordinal int NOT NULL, stack_count bigint NOT NULL CHECK(stack_count>0), buyout_total_copper bigint CHECK(buyout_total_copper>0), min_bid_copper bigint NOT NULL CHECK(min_bid_copper>=0), current_bid_copper bigint NOT NULL CHECK(current_bid_copper>=0), time_left_band int NOT NULL CHECK(time_left_band BETWEEN 1 AND 4), PRIMARY KEY(scan_id,ordinal));
CREATE TABLE item_observations (scan_id uuid NOT NULL REFERENCES scans, item_key text NOT NULL REFERENCES items, min_buyout jsonb, median_buyout jsonb, total_units bigint NOT NULL CHECK(total_units>=0), listing_count bigint, PRIMARY KEY(scan_id,item_key));
CREATE INDEX observations_item ON item_observations(item_key,scan_id);
CREATE TABLE historic_daily_imports (market_id uuid NOT NULL REFERENCES markets, item_key text NOT NULL REFERENCES items, utc_day bigint NOT NULL, source_identity text NOT NULL, point jsonb NOT NULL, PRIMARY KEY(market_id,item_key,utc_day,source_identity));
CREATE TABLE daily_item_metrics (market_id uuid NOT NULL REFERENCES markets, item_key text NOT NULL REFERENCES items, local_day date NOT NULL, method_version text NOT NULL, metrics jsonb NOT NULL, PRIMARY KEY(market_id,item_key,local_day));
CREATE TABLE watchlists (member_id uuid NOT NULL REFERENCES members, item_key text NOT NULL, PRIMARY KEY(member_id,item_key));
CREATE TABLE alert_rules (id uuid PRIMARY KEY, member_id uuid NOT NULL REFERENCES members, item_key text NOT NULL, kind text NOT NULL CHECK(kind IN ('underpriced','threshold','movement')), revision int NOT NULL DEFAULT 1, threshold_copper bigint CHECK(threshold_copper>0), discount_percent int NOT NULL DEFAULT 30 CHECK(discount_percent BETWEEN 1 AND 99), enabled boolean NOT NULL DEFAULT true, notify boolean NOT NULL DEFAULT false, created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE alert_events (id uuid PRIMARY KEY, rule_id uuid NOT NULL REFERENCES alert_rules, scan_id uuid NOT NULL REFERENCES scans, item_key text NOT NULL, observed_at timestamptz NOT NULL, evidence jsonb NOT NULL, dedupe_key text UNIQUE NOT NULL, created_at timestamptz NOT NULL DEFAULT now());
CREATE TABLE deliveries (id uuid PRIMARY KEY, event_id uuid NOT NULL UNIQUE REFERENCES alert_events, status text NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','sending','sent','failed','expired','uncertain')), attempt_count int NOT NULL DEFAULT 0, run_after timestamptz NOT NULL DEFAULT now(), last_error text, sent_at timestamptz);
CREATE TABLE jobs (id uuid PRIMARY KEY, job_type text NOT NULL, payload jsonb NOT NULL, status text NOT NULL DEFAULT 'pending', run_after timestamptz NOT NULL DEFAULT now(), lease_until timestamptz, attempt_count int NOT NULL DEFAULT 0);
CREATE TABLE audit_events (id bigserial PRIMARY KEY, actor_id uuid, action text NOT NULL, target_id uuid, at timestamptz NOT NULL DEFAULT now(), metadata jsonb NOT NULL DEFAULT '{}');
CREATE INDEX jobs_ready ON jobs(status,run_after);
CREATE INDEX uploads_pending ON uploads(status,received_at);
CREATE INDEX deliveries_ready ON deliveries(status,run_after);
CREATE TABLE rate_limits (key text NOT NULL, window_start timestamptz NOT NULL, count bigint NOT NULL, PRIMARY KEY(key,window_start));
