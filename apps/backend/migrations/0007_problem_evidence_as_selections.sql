-- A problem's evidence becomes a selection it cites, and problem_evidence goes away. Each
-- problem that had evidence gets one selection, "Evidence", holding those files as proof.

INSERT INTO selections (id, project_id, label, author, created_at)
    SELECT 'evidence-' || p.id, p.project_id, 'Evidence', p.author, p.created_at
    FROM problems p
    WHERE EXISTS (SELECT 1 FROM problem_evidence e WHERE e.problem_id = p.id);

INSERT INTO selection_items (project_id, selection_id, position, capture_id, artifact_id, mark)
    SELECT p.project_id, 'evidence-' || p.id,
           row_number() OVER (PARTITION BY e.problem_id ORDER BY e.artifact_id) - 1,
           a.capture_id, e.artifact_id, 'proof'
    FROM problem_evidence e
    JOIN problems p ON p.id = e.problem_id
    JOIN artifacts a ON a.id = e.artifact_id;

INSERT INTO problem_selections (project_id, problem_id, selection_id, position)
    SELECT project_id, substr(id, length('evidence-') + 1), id, 0
    FROM selections WHERE id LIKE 'evidence-%';

DROP INDEX idx_problem_evidence_artifact;
DROP TABLE problem_evidence;
