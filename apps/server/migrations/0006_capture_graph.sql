-- The capture graph: lineage, topics, surfaces, facets, proof selections, per-project blob
-- visibility, settings and tombstones.
--
-- Everything here is additive. `problem_evidence` and `fix_comparisons` stay until problems and
-- fixes cite selections; a later migration drops them.
--
-- Rows that must belong to one project carry `project_id` and reference their parent through
-- the pair (project_id, id), so the database refuses a link across projects without a trigger.
-- References that must keep a capture alive are NO ACTION (checked when the statement ends, so
-- deleting a whole project still cascades); a capture that something cites is tombstoned.

-- ---- captures --------------------------------------------------------------------------

-- `digest` is the SHA-256 of the canonical record the client wrote; `captured_at` is the
-- client's clock (created_at stays the time the server received it).
ALTER TABLE captures ADD COLUMN kind TEXT NOT NULL DEFAULT 'scenario'
    CHECK (kind IN ('scenario', 'derived'));
ALTER TABLE captures ADD COLUMN digest TEXT CHECK (digest IS NULL OR length(digest) = 64);
-- The line of captures this one continues: scenario and environment.
ALTER TABLE captures ADD COLUMN series TEXT;
ALTER TABLE captures ADD COLUMN captured_at INTEGER;
-- Changes with every edit of the lineage; an edit names the generation it read.
ALTER TABLE captures ADD COLUMN parents_generation INTEGER NOT NULL DEFAULT 0;
-- A removed capture stays as a tombstone so what cited it still resolves.
ALTER TABLE captures ADD COLUMN deleted_at INTEGER;
ALTER TABLE captures ADD COLUMN deleted_by TEXT;
ALTER TABLE captures ADD COLUMN delete_reason TEXT;

CREATE UNIQUE INDEX uq_captures_project_id ON captures(project_id, id);
CREATE INDEX idx_captures_series ON captures(project_id, series, created_at);

-- Files can be told apart from the same file in another capture by (capture, id).
CREATE UNIQUE INDEX uq_artifacts_capture_id ON artifacts(capture_id, id);

-- The logical screen a file shows, and where that screen is on its platform (JSON object:
-- a route, an activity, a window). `purged_at` marks bytes dropped after a removal.
CREATE TABLE surfaces (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    key TEXT NOT NULL CHECK (length(key) BETWEEN 1 AND 128),
    label TEXT,
    UNIQUE (project_id, key)
);
ALTER TABLE artifacts ADD COLUMN surface_id TEXT REFERENCES surfaces(id) ON DELETE SET NULL;
ALTER TABLE artifacts ADD COLUMN locator TEXT CHECK (locator IS NULL OR json_valid(locator));
ALTER TABLE artifacts ADD COLUMN purged_at INTEGER;
CREATE INDEX idx_artifacts_surface ON artifacts(surface_id, capture_id);

-- The environment flattened to dotted keys (`appearance.color_scheme`), to search captures by
-- any of them.
CREATE TABLE capture_facets (
    project_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL,
    capture_id TEXT NOT NULL,
    PRIMARY KEY (project_id, key, value, capture_id),
    FOREIGN KEY (project_id, capture_id) REFERENCES captures(project_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_capture_facets_capture ON capture_facets(capture_id);

-- ---- blobs -----------------------------------------------------------------------------

-- Bytes are stored once per digest. A project only knows the blobs it has uploaded or cites,
-- which is what `blobs/missing` answers from.
CREATE TABLE blobs (
    sha256 TEXT PRIMARY KEY CHECK (length(sha256) = 64),
    bytes INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE project_blobs (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    sha256 TEXT NOT NULL REFERENCES blobs(sha256) ON DELETE CASCADE,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, sha256)
);
CREATE INDEX idx_project_blobs_sha256 ON project_blobs(sha256);

INSERT INTO blobs (sha256, bytes, created_at)
    SELECT a.sha256, MAX(a.bytes), MIN(c.created_at)
    FROM artifacts a JOIN captures c ON c.id = a.capture_id
    GROUP BY a.sha256;
INSERT INTO project_blobs (project_id, sha256, created_at)
    SELECT c.project_id, a.sha256, MIN(c.created_at)
    FROM artifacts a JOIN captures c ON c.id = a.capture_id
    GROUP BY c.project_id, a.sha256;

-- ---- lineage ---------------------------------------------------------------------------

-- The lineage that holds today: the parents the record declared (origin 'record') or an
-- edit set later (origin 'edit'). A parent cannot be removed while a child points at it.
CREATE TABLE capture_parents (
    project_id TEXT NOT NULL,
    capture_id TEXT NOT NULL,
    parent_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('previous', 'baseline', 'input')),
    position INTEGER NOT NULL DEFAULT 0,
    origin TEXT NOT NULL CHECK (origin IN ('record', 'edit')),
    PRIMARY KEY (capture_id, parent_id),
    CHECK (capture_id <> parent_id),
    FOREIGN KEY (project_id, capture_id) REFERENCES captures(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, parent_id) REFERENCES captures(project_id, id)
);
CREATE INDEX idx_capture_parents_parent ON capture_parents(parent_id);

-- An edge may not close a loop: the parent's ancestors must not include the capture.
CREATE TRIGGER capture_parents_no_cycle BEFORE INSERT ON capture_parents
BEGIN
    SELECT RAISE(ABORT, 'capture lineage cycle')
    WHERE NEW.capture_id IN (
        WITH RECURSIVE up(id) AS (
            SELECT NEW.parent_id
            UNION
            SELECT p.parent_id FROM capture_parents p JOIN up ON p.capture_id = up.id)
        SELECT id FROM up);
END;

-- Who changed the lineage, from what to what, and why.
CREATE TABLE capture_parent_events (
    id TEXT PRIMARY KEY,
    capture_id TEXT NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
    generation INTEGER NOT NULL,
    actor TEXT NOT NULL,
    reason TEXT NOT NULL,
    before TEXT NOT NULL CHECK (json_valid(before)),
    after TEXT NOT NULL CHECK (json_valid(after)),
    created_at INTEGER NOT NULL
);
CREATE INDEX idx_capture_parent_events_capture ON capture_parent_events(capture_id, generation);

-- ---- topics ----------------------------------------------------------------------------

-- A tree of subjects. Names are unique among siblings, ignoring case; the root level is
-- unique per project. Membership is inherited by the subtree when queried, never stored.
CREATE TABLE topics (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    parent_id TEXT,
    name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 64 AND instr(name, '/') = 0),
    position INTEGER NOT NULL DEFAULT 0,
    -- Changes with every rename or move; a change names the generation it read.
    generation INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    CHECK (parent_id IS NULL OR parent_id <> id),
    FOREIGN KEY (project_id, parent_id) REFERENCES topics(project_id, id)
);
CREATE UNIQUE INDEX uq_topics_project_id ON topics(project_id, id);
-- SQLite treats NULLs as distinct in a UNIQUE index, so roots get an index of their own.
CREATE UNIQUE INDEX uq_topics_root_name ON topics(project_id, lower(name)) WHERE parent_id IS NULL;
CREATE UNIQUE INDEX uq_topics_child_name ON topics(parent_id, lower(name)) WHERE parent_id IS NOT NULL;
CREATE INDEX idx_topics_parent ON topics(parent_id, position, name);

-- A move may not put a topic under itself or one of its descendants.
CREATE TRIGGER topics_no_cycle BEFORE UPDATE OF parent_id ON topics
WHEN NEW.parent_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'topic cycle')
    WHERE NEW.id IN (
        WITH RECURSIVE up(id) AS (
            SELECT NEW.parent_id
            UNION
            SELECT t.parent_id FROM topics t JOIN up ON t.id = up.id WHERE t.parent_id IS NOT NULL)
        SELECT id FROM up);
END;

CREATE TABLE capture_topics (
    project_id TEXT NOT NULL,
    topic_id TEXT NOT NULL,
    capture_id TEXT NOT NULL,
    added_by TEXT,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (topic_id, capture_id),
    FOREIGN KEY (project_id, topic_id) REFERENCES topics(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, capture_id) REFERENCES captures(project_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_capture_topics_capture ON capture_topics(capture_id);

-- ---- proof -----------------------------------------------------------------------------

-- A frozen list of anchors into captures. A problem or a fix cites selections; changing the
-- proof makes a new selection and leaves the old one as history.
CREATE TABLE selections (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    author TEXT,
    created_at INTEGER NOT NULL,
    UNIQUE (project_id, id)
);

-- An anchor is a whole capture (artifact_id NULL) or one of its files, marked as proof or as
-- context, with an optional fragment (a time span of a video, a region of an image).
CREATE TABLE selection_items (
    project_id TEXT NOT NULL,
    selection_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    capture_id TEXT NOT NULL,
    artifact_id TEXT,
    mark TEXT NOT NULL CHECK (mark IN ('proof', 'context')),
    fragment TEXT CHECK (fragment IS NULL OR json_valid(fragment)),
    PRIMARY KEY (selection_id, position),
    FOREIGN KEY (project_id, selection_id) REFERENCES selections(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, capture_id) REFERENCES captures(project_id, id),
    -- The file must be one of the cited capture's.
    FOREIGN KEY (capture_id, artifact_id) REFERENCES artifacts(capture_id, id)
);
CREATE INDEX idx_selection_items_capture ON selection_items(capture_id);
CREATE INDEX idx_selection_items_artifact ON selection_items(artifact_id);

CREATE UNIQUE INDEX uq_problems_project_id ON problems(project_id, id);
CREATE UNIQUE INDEX uq_fixes_project_id ON fixes(project_id, id);

CREATE TABLE problem_selections (
    project_id TEXT NOT NULL,
    problem_id TEXT NOT NULL,
    selection_id TEXT NOT NULL,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (problem_id, selection_id),
    FOREIGN KEY (project_id, problem_id) REFERENCES problems(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, selection_id) REFERENCES selections(project_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_problem_selections_selection ON problem_selections(selection_id);

-- A fix has the selection it started from and the ones that prove the solution.
CREATE TABLE fix_selections (
    project_id TEXT NOT NULL,
    fix_id TEXT NOT NULL,
    selection_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('baseline', 'solution-proof')),
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (fix_id, selection_id),
    FOREIGN KEY (project_id, fix_id) REFERENCES fixes(project_id, id) ON DELETE CASCADE,
    FOREIGN KEY (project_id, selection_id) REFERENCES selections(project_id, id) ON DELETE CASCADE
);
CREATE INDEX idx_fix_selections_selection ON fix_selections(selection_id);

-- ---- settings and audit ----------------------------------------------------------------

-- Editable from the panel. A server setting overrides the environment variable it started
-- from; a project setting may only tighten a server limit. Values are JSON.
CREATE TABLE server_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL CHECK (json_valid(value)),
    generation INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT,
    updated_at INTEGER NOT NULL
);
CREATE TABLE project_settings (
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    value TEXT NOT NULL CHECK (json_valid(value)),
    generation INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, key)
);

-- What an event touched, as JSON: the citations of a removed capture, and why.
ALTER TABLE audit_events ADD COLUMN detail TEXT CHECK (detail IS NULL OR json_valid(detail));

-- ---- indexes the older tables lack -----------------------------------------------------

-- Without them, asking who cites an artifact scans the whole table.
CREATE INDEX idx_problem_evidence_artifact ON problem_evidence(artifact_id);
CREATE INDEX idx_fix_comparisons_before ON fix_comparisons(before_artifact_id);
CREATE INDEX idx_fix_comparisons_after ON fix_comparisons(after_artifact_id);
CREATE INDEX idx_fix_comparisons_diff ON fix_comparisons(diff_artifact_id);
CREATE INDEX idx_fix_steps_evidence ON fix_steps(evidence_artifact_id);
CREATE INDEX idx_design_entries_evidence ON design_entries(evidence);
CREATE INDEX idx_capture_findings_capture ON capture_findings(capture_id);
CREATE INDEX idx_capture_findings_evidence ON capture_findings(evidence_artifact_id);
