ALTER TABLE scans ADD COLUMN realm_explicit boolean NOT NULL DEFAULT false;
ALTER TABLE historic_daily_imports ADD COLUMN realm_explicit boolean NOT NULL DEFAULT false;
ALTER TABLE historic_daily_imports ADD COLUMN received_at timestamptz NOT NULL DEFAULT now();
CREATE TABLE export_history_fingerprints (
 market_id uuid NOT NULL REFERENCES markets, kind text NOT NULL CHECK(kind IN ('recent','daily')),
 fingerprint text NOT NULL, exported_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY(market_id,kind,fingerprint)
);
-- Full scans have always required explicit matching realm metadata at ingestion.
UPDATE scans SET realm_explicit=true WHERE source_kind='full_scan';
