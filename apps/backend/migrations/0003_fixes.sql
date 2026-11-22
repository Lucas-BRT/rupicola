-- Fixes: changes that answer one or more problems, with before/after proof and review.
-- Creating a fix puts its problems in progress; approving it resolves them.

CREATE TABLE fixes (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    summary TEXT NOT NULL,
    status TEXT NOT NULL
        CHECK (status IN ('pending-review', 'approved', 'changes-requested', 'superseded')),
    author TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_fixes_project ON fixes(project_id, updated_at);

CREATE TABLE fix_problems (
    fix_id TEXT NOT NULL REFERENCES fixes(id) ON DELETE CASCADE,
    problem_id TEXT NOT NULL REFERENCES problems(id) ON DELETE CASCADE,
    PRIMARY KEY (fix_id, problem_id)
);

CREATE INDEX idx_fix_problems_problem ON fix_problems(problem_id);

-- Design tactics applied and files changed, as free-form labels.
CREATE TABLE fix_tactics (
    fix_id TEXT NOT NULL REFERENCES fixes(id) ON DELETE CASCADE,
    tactic TEXT NOT NULL,
    PRIMARY KEY (fix_id, tactic)
);

CREATE TABLE fix_files (
    fix_id TEXT NOT NULL REFERENCES fixes(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    PRIMARY KEY (fix_id, path)
);

-- Before/after proof: artifacts of captures in the same project.
CREATE TABLE fix_comparisons (
    id TEXT PRIMARY KEY,
    fix_id TEXT NOT NULL REFERENCES fixes(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    label TEXT NOT NULL,
    route TEXT NOT NULL,
    viewport TEXT NOT NULL,
    theme TEXT NOT NULL,
    before_artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    after_artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
    diff_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    changed_ratio REAL
);

CREATE INDEX idx_fix_comparisons_fix ON fix_comparisons(fix_id, position);

CREATE TABLE fix_steps (
    id TEXT PRIMARY KEY,
    fix_id TEXT NOT NULL REFERENCES fixes(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    title TEXT NOT NULL,
    detail TEXT NOT NULL,
    verification TEXT NOT NULL,
    evidence_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_fix_steps_fix ON fix_steps(fix_id, position);

CREATE TABLE fix_reviews (
    id TEXT PRIMARY KEY,
    fix_id TEXT NOT NULL REFERENCES fixes(id) ON DELETE CASCADE,
    author TEXT,
    outcome TEXT NOT NULL
        CHECK (outcome IN ('pending-review', 'approved', 'changes-requested', 'superseded')),
    comment TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_fix_reviews_fix ON fix_reviews(fix_id, created_at);
