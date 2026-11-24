-- Design records: briefs, directions, tactics and decisions of a project.

CREATE TABLE design_entries (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('brief', 'direction', 'tactic', 'decision')),
    status TEXT NOT NULL CHECK (status IN ('proposed', 'adopted', 'rejected')),
    title TEXT NOT NULL,
    note TEXT NOT NULL,
    surface TEXT,
    supersedes TEXT REFERENCES design_entries(id) ON DELETE SET NULL,
    reference TEXT,
    evidence TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    author TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_design_project ON design_entries(project_id, created_at);
