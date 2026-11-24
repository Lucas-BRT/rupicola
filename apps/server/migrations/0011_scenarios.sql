-- Capture definitions a repository owns, pushed to the server and kept by the hash of the parsed
-- definition. Pushing the same definition again only makes it the latest again; a project's
-- scenario is its most recently pushed version.
CREATE TABLE scenario_versions (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    hash TEXT NOT NULL,
    definition TEXT NOT NULL,
    author TEXT,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, name, hash)
);

CREATE INDEX idx_scenario_versions_latest ON scenario_versions(project_id, name, created_at DESC);
