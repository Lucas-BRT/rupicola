CREATE INDEX IF NOT EXISTS idx_members_handle ON members(handle);
CREATE INDEX IF NOT EXISTS idx_tokens_project ON tokens(project_id);
CREATE INDEX IF NOT EXISTS idx_tokens_expires ON tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_invites_project_expires ON invites(project_id, expires_at);
CREATE INDEX IF NOT EXISTS idx_artifacts_capture_role_pos ON artifacts(capture_id, role, position);
