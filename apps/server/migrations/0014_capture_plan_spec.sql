ALTER TABLE captures ADD COLUMN plan_json TEXT;
ALTER TABLE captures ADD COLUMN plan_digest TEXT;
CREATE INDEX IF NOT EXISTS idx_captures_plan_digest ON captures(project_id, plan_digest);

ALTER TABLE capture_jobs ADD COLUMN plan_spec_json TEXT;
ALTER TABLE capture_jobs ADD COLUMN parent_capture_id TEXT;

ALTER TABLE fixes ADD COLUMN mismatch_override_reason TEXT;
