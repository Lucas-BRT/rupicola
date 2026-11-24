-- Problems: defects found in a project's interface, with the evidence and rules behind
-- them. Evidence points at captured artifacts, so it follows the artifact's lifetime.

CREATE TABLE problems (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    status TEXT NOT NULL
        CHECK (status IN ('open', 'in-progress', 'intentional', 'false-positive',
                          'needs-reprocess', 'resolved')),
    severity TEXT NOT NULL CHECK (severity IN ('critical', 'major', 'minor', 'polish')),
    origin TEXT NOT NULL CHECK (origin IN ('agent-measured', 'agent-visual', 'user-reported')),
    route TEXT NOT NULL,
    viewport TEXT NOT NULL,
    theme TEXT NOT NULL,
    diagnosis TEXT NOT NULL,
    proposed_solution TEXT NOT NULL,
    author TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_problems_project ON problems(project_id, updated_at);

-- Design rules a problem violates.
CREATE TABLE problem_rules (
    problem_id TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
    scope TEXT NOT NULL,
    code TEXT NOT NULL,
    title TEXT NOT NULL,
    PRIMARY KEY (problem_id, scope, code)
);

CREATE TABLE problem_evidence (
    problem_id TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
    artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    PRIMARY KEY (problem_id, artifact_id)
);

CREATE TABLE problem_comments (
    id TEXT PRIMARY KEY,
    problem_id TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
    author TEXT,
    text TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_problem_comments_problem ON problem_comments(problem_id, created_at);
