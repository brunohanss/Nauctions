ALTER TABLE alert_rules ADD COLUMN deleted_at timestamptz;
CREATE INDEX active_rules_member ON alert_rules(member_id) WHERE deleted_at IS NULL;
