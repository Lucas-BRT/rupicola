-- Local development database seed for Rupicola (SQLite).
-- Default credentials: handle = "demo", password = "demo-password-1234"
--
-- Usage (after rupicola-server has run migrations on the target database):
--   sqlite3 .rupicola/panel.db < apps/server/seed-local.sql
-- Or with Python (no sqlite3 binary needed):
--   python3 -c 'import sqlite3; c=sqlite3.connect(".rupicola/panel.db"); c.executescript(open("apps/server/seed-local.sql").read()); c.commit()'

PRAGMA foreign_keys = ON;

BEGIN;

-- 1. Default administrator account ("demo" / "demo-password-1234")
INSERT OR IGNORE INTO accounts (handle, password_hash, admin, created_at)
VALUES (
    'demo',
    '$argon2id$v=19$m=19456,t=2,p=1$QbMuJPAUC/b2eCDzVdQsgA$c/g10Fp+ZDELnIn6OSuJNVX1Cu9x7d2I8oLMYxxtiec',
    1,
    1763950000000
);

-- 2. Mock projects
INSERT OR IGNORE INTO projects (id, label, created_at) VALUES
    ('atelier-checkout', 'Atelier Checkout', 1763950010000),
    ('cosmic-notes', 'Cosmic Notes (Desktop)', 1763950020000),
    ('docs-portal', 'Docs Portal', 1763950030000);

INSERT OR IGNORE INTO members (project_id, handle, role) VALUES
    ('atelier-checkout', 'demo', 'owner'),
    ('cosmic-notes', 'demo', 'owner'),
    ('docs-portal', 'demo', 'owner');

-- 3. Design records
INSERT OR IGNORE INTO design_entries (
    id, project_id, kind, status, title, note, surface, supersedes, reference, evidence, author, created_at
) VALUES
    (
        '01950000-0000-7000-8000-000000000101',
        'atelier-checkout',
        'brief',
        'adopted',
        'Checkout & billing settings QA workspace',
        'Track desktop (1440x900) and mobile (320x844) flows before every release.',
        'panel + checkout',
        NULL,
        NULL,
        NULL,
        'demo',
        1763950100000
    ),
    (
        '01950000-0000-7000-8000-000000000102',
        'atelier-checkout',
        'direction',
        'adopted',
        'Guianan Crest palette (#FB6803) on neutral graphite',
        'Use cold graphite (#09090B / #131317) or crisp porcelain (#FAFAFA / #FFFFFF) with #FB6803 reserved for primary actions and focus indicators.',
        'global',
        NULL,
        NULL,
        NULL,
        'demo',
        1763950110000
    ),
    (
        '01950000-0000-7000-8000-000000000103',
        'atelier-checkout',
        'tactic',
        'adopted',
        'Restore trigger focus on Escape',
        'Popovers and dialogs must return keyboard focus to their opener on Escape.',
        '/settings',
        NULL,
        NULL,
        NULL,
        'demo',
        1763950120000
    ),
    (
        '01950000-0000-7000-8000-000000000201',
        'cosmic-notes',
        'brief',
        'adopted',
        'Tier 1 offscreen RAM capture for libcosmic views',
        'Capture deterministic timestamps directly into tiny_skia pixel buffers.',
        'desktop',
        NULL,
        NULL,
        NULL,
        'demo',
        1763950130000
    );

-- 4. Mock captures (complete, reviewed, and blocked states)
INSERT OR IGNORE INTO captures (
    id, project_id, scenario, revision, status, blocker, inspection, environment, created_at
) VALUES
    (
        '01950000-0000-7000-8000-000000001001',
        'atelier-checkout',
        'settings-popover',
        'rev-710b',
        'complete',
        NULL,
        'reviewed',
        '{"width":1440,"height":900,"color_scheme":"dark","reduced_motion":false,"renderer":"chromium-headless"}',
        1763950200000
    ),
    (
        '01950000-0000-7000-8000-000000001002',
        'atelier-checkout',
        'settings-popover',
        'rev-a91f02',
        'complete',
        NULL,
        'pending',
        '{"width":1440,"height":900,"color_scheme":"dark","reduced_motion":false,"renderer":"chromium-headless"}',
        1763950300000
    ),
    (
        '01950000-0000-7000-8000-000000001003',
        'atelier-checkout',
        'billing-modal-focus',
        'rev-84c002',
        'blocked',
        'Preview URL http://127.0.0.1:4321 was not reachable from the server network.',
        'pending',
        '{"width":320,"height":844,"color_scheme":"light","reduced_motion":false,"renderer":"chromium-headless"}',
        1763950400000
    ),
    (
        '01950000-0000-7000-8000-000000001004',
        'cosmic-notes',
        'sidebar-workspace',
        'rev-19a',
        'complete',
        NULL,
        'reviewed',
        '{"width":1280,"height":800,"color_scheme":"dark","reduced_motion":true,"renderer":"tiny-skia"}',
        1763950500000
    ),
    (
        '01950000-0000-7000-8000-000000001005',
        'docs-portal',
        'docs-overview',
        'rev-main',
        'complete',
        NULL,
        'reviewed',
        '{"width":1440,"height":900,"color_scheme":"dark","reduced_motion":false,"renderer":"chromium-headless"}',
        1763950600000
    );

INSERT OR IGNORE INTO capture_findings (id, capture_id, note, evidence_artifact_id, author, created_at) VALUES
    (
        '01950000-0000-7000-8000-000000002001',
        '01950000-0000-7000-8000-000000001001',
        'Escape closes the theme popover without returning keyboard focus to the trigger button.',
        NULL,
        'demo',
        1763950210000
    );

-- 5. Problems
INSERT OR IGNORE INTO problems (
    id, project_id, title, status, severity, origin, route, viewport, theme,
    diagnosis, proposed_solution, author, created_at, updated_at
) VALUES
    (
        '01950000-0000-7000-8000-000000003001',
        'atelier-checkout',
        'Popover loses focus when dismissed with Escape',
        'in-progress',
        'critical',
        'agent-visual',
        '/settings',
        '1440x900',
        'dark',
        'Keydown handler closes the dialog without restoring focus to the opener button.',
        'Capture document.activeElement before showModal() and call opener?.focus() on close.',
        'demo',
        1763950700000,
        1763950800000
    ),
    (
        '01950000-0000-7000-8000-000000003002',
        'atelier-checkout',
        'Summary table overflows 320px viewport horizontally',
        'open',
        'major',
        'agent-measured',
        '/checkout',
        '320x844',
        'light',
        'Unconstrained flex children cause horizontal scroll on narrow mobile screens.',
        'Add min-inline-size: 0 on grid items and wrap the summary table container.',
        'demo',
        1763950750000,
        1763950750000
    ),
    (
        '01950000-0000-7000-8000-000000003003',
        'cosmic-notes',
        'Sidebar divider contrast drops below 3:1 in dark theme',
        'open',
        'minor',
        'agent-visual',
        'sidebar',
        '1280x800',
        'dark',
        '1px separator blends into surface background under low ambient light.',
        'Raise hairline border luminance to #232329.',
        'demo',
        1763950780000,
        1763950780000
    );

INSERT OR IGNORE INTO problem_rules (problem_id, scope, code, title) VALUES
    ('01950000-0000-7000-8000-000000003001', 'global', 'focus-restore', 'Restore focus to opener on modal Escape');

INSERT OR IGNORE INTO problem_comments (id, problem_id, author, text, created_at) VALUES
    ('01950000-0000-7000-8000-000000003501', '01950000-0000-7000-8000-000000003001', 'demo', 'Reproduced with keyboard-only navigation in Chromium 159.', 1763950790000);

-- 6. Fixes
INSERT OR IGNORE INTO fixes (id, project_id, title, summary, status, author, created_at, updated_at) VALUES
    (
        '01950000-0000-7000-8000-000000004001',
        'atelier-checkout',
        'Restore trigger focus on Escape and align 1px hairline grid',
        'Restored focus to opener on Escape and enforced 2px #FB6803 focus-visible ring.',
        'pending-review',
        'demo',
        1763950800000,
        1763950800000
    );

INSERT OR IGNORE INTO fix_problems (fix_id, problem_id) VALUES
    ('01950000-0000-7000-8000-000000004001', '01950000-0000-7000-8000-000000003001');

INSERT OR IGNORE INTO fix_tactics (fix_id, tactic) VALUES
    ('01950000-0000-7000-8000-000000004001', 'focus-restore'),
    ('01950000-0000-7000-8000-000000004001', 'focus-visible');

INSERT OR IGNORE INTO fix_files (fix_id, path) VALUES
    ('01950000-0000-7000-8000-000000004001', 'apps/panel/assets/panel.js'),
    ('01950000-0000-7000-8000-000000004001', 'apps/panel/assets/panel.css');

INSERT OR IGNORE INTO fix_steps (id, fix_id, position, title, detail, verification, evidence_artifact_id, created_at) VALUES
    (
        '01950000-0000-7000-8000-000000004101',
        '01950000-0000-7000-8000-000000004001',
        0,
        'Track opener before opening dialog and refocus on close',
        'Stored document.activeElement before showModal() and restored focus inside close listener.',
        'Verified Tab -> Enter -> Escape keyboard path.',
        NULL,
        1763950810000
    );

-- 7. Audit events
INSERT OR IGNORE INTO audit_events (id, project_id, actor, event, occurred_at, detail) VALUES
    ('01950000-0000-7000-8000-000000005001', 'atelier-checkout', 'demo', 'Created the project', 1763950010000, NULL),
    ('01950000-0000-7000-8000-000000005002', 'cosmic-notes', 'demo', 'Created the project', 1763950020000, NULL),
    ('01950000-0000-7000-8000-000000005003', 'docs-portal', 'demo', 'Created the project', 1763950030000, NULL);

COMMIT;
