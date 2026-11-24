-- A fix's comparisons say which tree they were captured on, so one fix's proof is not mixed with
-- another fix's changes.
--
-- view: 'isolated' is the base against the base plus this fix alone; 'integrated' is the base
-- with every approved fix against the same plus this fix. base_revision and revision name the
-- source revisions of the before and after captures. applied_fixes lists the fixes (JSON array
-- of ids) that the integrated before tree already had, which is how a comparison is found stale
-- when the approved set changes.
ALTER TABLE fix_comparisons ADD COLUMN view TEXT NOT NULL DEFAULT 'isolated';
ALTER TABLE fix_comparisons ADD COLUMN base_revision TEXT NOT NULL DEFAULT '';
ALTER TABLE fix_comparisons ADD COLUMN revision TEXT NOT NULL DEFAULT '';
ALTER TABLE fix_comparisons ADD COLUMN applied_fixes TEXT NOT NULL DEFAULT '[]';
