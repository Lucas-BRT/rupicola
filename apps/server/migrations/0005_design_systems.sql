-- Design systems: versions of a project's tokens and guidance, and which one is adopted.

CREATE TABLE design_versions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    parent_id TEXT,
    author TEXT NOT NULL,
    phase TEXT NOT NULL CHECK (phase IN ('draft', 'review', 'approved', 'changes')),
    -- Changes with every write; a write names the generation it read.
    generation INTEGER NOT NULL DEFAULT 0,
    name TEXT NOT NULL,
    document TEXT NOT NULL CHECK (json_valid(document)),
    notes TEXT NOT NULL CHECK (json_valid(notes)),
    -- The last validation, with its screenshots.
    report TEXT CHECK (report IS NULL OR json_valid(report)),
    -- The examples as they were when the version was sent for review.
    frozen_screen TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_design_versions_project ON design_versions(project_id, created_at);

CREATE TABLE design_adoption (
    project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
    version_id TEXT NOT NULL REFERENCES design_versions(id)
);
