-- Requests to run a scenario on trees of the code, taken by a runner. A runner claims a job and
-- holds it for as long as it keeps reporting; a job whose lease runs out is claimable again.
CREATE TABLE capture_jobs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    scenario TEXT NOT NULL,
    hash TEXT NOT NULL,
    fix_id TEXT REFERENCES fixes(id) ON DELETE SET NULL,
    trees TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'queued',
    cells_total INTEGER NOT NULL,
    cells_done INTEGER NOT NULL DEFAULT 0,
    runner TEXT,
    runner_key TEXT,
    lease_expires_at INTEGER,
    error TEXT,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    finished_at INTEGER
);

CREATE INDEX idx_capture_jobs_queue ON capture_jobs(project_id, status, created_at);
CREATE INDEX idx_capture_jobs_fix ON capture_jobs(fix_id);

-- What a finished job made, by tree, viewport and theme.
CREATE TABLE capture_job_captures (
    job_id TEXT NOT NULL REFERENCES capture_jobs(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    tree TEXT NOT NULL,
    viewport TEXT NOT NULL,
    theme TEXT NOT NULL,
    PRIMARY KEY (job_id, position)
);

CREATE INDEX idx_capture_job_captures_capture ON capture_job_captures(capture_id);
