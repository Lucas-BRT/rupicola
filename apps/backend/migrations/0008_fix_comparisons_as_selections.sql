-- A fix's comparisons also become the selections it cites: the before files as its baseline,
-- the after and diff files as proof that it works. fix_comparisons stays as the pairing of
-- those files by route, viewport and theme.

INSERT INTO selections (id, project_id, label, author, created_at)
    SELECT 'before-' || f.id, f.project_id, 'Before', f.author, f.created_at
    FROM fixes f WHERE EXISTS (SELECT 1 FROM fix_comparisons c WHERE c.fix_id = f.id);

INSERT INTO selection_items (project_id, selection_id, position, capture_id, artifact_id, mark)
    SELECT f.project_id, 'before-' || f.id,
           row_number() OVER (PARTITION BY c.fix_id ORDER BY c.position) - 1,
           a.capture_id, c.before_artifact_id, 'proof'
    FROM fix_comparisons c
    JOIN fixes f ON f.id = c.fix_id
    JOIN artifacts a ON a.id = c.before_artifact_id;

INSERT INTO selections (id, project_id, label, author, created_at)
    SELECT 'after-' || f.id, f.project_id, 'After', f.author, f.created_at
    FROM fixes f WHERE EXISTS (SELECT 1 FROM fix_comparisons c WHERE c.fix_id = f.id);

INSERT INTO selection_items (project_id, selection_id, position, capture_id, artifact_id, mark)
    SELECT f.project_id, 'after-' || f.id,
           row_number() OVER (PARTITION BY files.fix_id ORDER BY files.ord) - 1,
           a.capture_id, files.artifact, 'proof'
    FROM (
        SELECT fix_id, position * 2 AS ord, after_artifact_id AS artifact FROM fix_comparisons
        UNION ALL
        SELECT fix_id, position * 2 + 1, diff_artifact_id FROM fix_comparisons
        WHERE diff_artifact_id IS NOT NULL
    ) files
    JOIN fixes f ON f.id = files.fix_id
    JOIN artifacts a ON a.id = files.artifact;

INSERT INTO fix_selections (project_id, fix_id, selection_id, role, position)
    SELECT project_id, substr(id, length('before-') + 1), id, 'baseline', 0
    FROM selections WHERE id LIKE 'before-%';
INSERT INTO fix_selections (project_id, fix_id, selection_id, role, position)
    SELECT project_id, substr(id, length('after-') + 1), id, 'solution-proof', 1
    FROM selections WHERE id LIKE 'after-%';
