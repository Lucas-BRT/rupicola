-- Initial schema. Conventions:
--   * ids are UUIDv7 text (time ordered) unless the natural key is a handle or digest;
--   * every instant is Unix milliseconds in an INTEGER column named *_at;
--   * enumerations are TEXT with a CHECK listing the allowed values;
--   * foreign keys cascade; the backend turns them on for every connection.
-- Credentials are stored only as the SHA-256 digest of their secret.

CREATE TABLE accounts (
    handle TEXT PRIMARY KEY CHECK (length(handle) BETWEEN 3 AND 48),
    password_hash TEXT NOT NULL,
    -- Administers the whole server: sees and manages every project.
    admin INTEGER NOT NULL DEFAULT 0 CHECK (admin IN (0, 1)),
    created_at INTEGER NOT NULL
);

CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    label TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE members (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    handle TEXT NOT NULL REFERENCES accounts(handle) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'reviewer', 'reader')),
    PRIMARY KEY (project_id, handle)
);

CREATE TABLE invites (
    digest TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('owner', 'reviewer', 'reader')),
    created_by TEXT REFERENCES accounts(handle) ON DELETE SET NULL,
    expires_at INTEGER NOT NULL
);

-- Every credential: a person's session, an agent acting for a person (machine),
-- or a capture device bound to one project.
CREATE TABLE tokens (
    digest TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('session', 'machine', 'device')),
    handle TEXT REFERENCES accounts(handle) ON DELETE CASCADE,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    last_used_at INTEGER,
    CHECK (
        (kind = 'device' AND project_id IS NOT NULL AND handle IS NULL)
        OR (kind <> 'device' AND handle IS NOT NULL AND project_id IS NULL)
    )
);

CREATE INDEX idx_tokens_handle ON tokens(handle);

CREATE TABLE audit_events (
    id TEXT PRIMARY KEY,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    actor TEXT NOT NULL,
    event TEXT NOT NULL,
    occurred_at INTEGER NOT NULL
);

CREATE INDEX idx_audit_events_project ON audit_events(project_id, occurred_at);

-- One capture run. Its media files are artifacts; their bytes live in the blob
-- store, addressed by sha256.
CREATE TABLE captures (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    scenario TEXT NOT NULL,
    revision TEXT,
    status TEXT NOT NULL CHECK (status IN ('complete', 'blocked')),
    blocker TEXT,
    inspection TEXT NOT NULL,
    -- Viewport, scale, color scheme, renderer… as captured (JSON object).
    environment TEXT NOT NULL CHECK (json_valid(environment)),
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_captures_project ON captures(project_id, created_at);

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    role TEXT NOT NULL
        CHECK (role IN ('checkpoint', 'clip-video', 'clip-sheet', 'clip-analysis', 'clip-frame')),
    -- Checkpoint or clip the artifact belongs to.
    owner_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL CHECK (length(sha256) = 64),
    bytes INTEGER NOT NULL,
    mime TEXT NOT NULL,
    target_ms REAL,
    video_ms REAL,
    UNIQUE (capture_id, path)
);

CREATE INDEX idx_artifacts_sha256 ON artifacts(sha256);

-- Review notes left on a capture.
CREATE TABLE capture_findings (
    id TEXT PRIMARY KEY,
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    note TEXT NOT NULL,
    evidence_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    author TEXT,
    created_at INTEGER NOT NULL
);
