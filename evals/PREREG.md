# Pre-registration template

Copy to `evals/prereg/<date>-<phase>.md` and commit it before running any condition. A commit that precedes the result files is the pre-registration.

- **Phase and change under test:**
- **Conditions:** C0 no skill, C1 baseline tag, C2 candidate commit (SHA)
- **Model IDs and harness versions (fixed for the whole run):**
- **Task set and split:** ids, dev or holdout. Holdout runs only at milestones.
- **Repetitions per task (k):**
- **Primary metric and hypothesis:** e.g. pass, C2 minus C1 at least +10 pp
- **Test:** mixed-effects logistic regression, `pass ~ condition + (1|task) + (1|harness)`; paired cluster bootstrap, 10000 resamples, 95% CI
- **Margin for non-inferiority (if any):**
- **Secondary metrics and correction:** Holm-Bonferroni across the family
- **Stopping rule and what counts as a revert:** any significant drop in pass or rise in constraint_violation
- **Power calculation:** discordance estimate source, alpha, power, resulting pairs
