-- Beta observations remain separate from live scans, baselines, and alerts.
CREATE TABLE forecast_beta_runs (
    market_id uuid NOT NULL REFERENCES markets(id),
    run_id text NOT NULL,
    payload jsonb NOT NULL,
    imported_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (market_id, run_id)
);
CREATE TABLE forecast_launch_settings (
    market_id uuid PRIMARY KEY REFERENCES markets(id),
    started_at timestamptz NOT NULL
);
