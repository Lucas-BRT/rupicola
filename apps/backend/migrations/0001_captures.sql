-- Captures and their media artifacts. Media bytes live in the blob store,
-- addressed by sha256; these rows only describe them.

CREATE TABLE captures (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    project_label TEXT NOT NULL,
    scenario TEXT NOT NULL,
    revision TEXT,
    status TEXT NOT NULL,
    blocker TEXT,
    inspection TEXT NOT NULL,
    environment_json TEXT NOT NULL,
    findings_json TEXT NOT NULL,
    created_at_ms INTEGER NOT NULL,
    legacy_root TEXT,
    legacy_name TEXT
);

CREATE INDEX idx_captures_project ON captures(project_id, created_at_ms);

CREATE UNIQUE INDEX idx_captures_legacy ON captures(legacy_root, legacy_name);

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    capture_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    role TEXT NOT NULL,
    owner_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    bytes INTEGER NOT NULL,
    mime TEXT NOT NULL,
    target_ms REAL,
    video_ms REAL
);

CREATE UNIQUE INDEX idx_artifacts_path ON artifacts(capture_id, path);

CREATE INDEX idx_artifacts_sha ON artifacts(sha256);
