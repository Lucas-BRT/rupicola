//! Validation and translation of [`CaptureStep`] sequences and [`Scenario`] matrix cells
//! into declarative `rupicola_capture::session::Plan` plans.

use crate::{CaptureAction, CaptureStep, Checks, Scenario, Viewport};
use rupicola_capture::session::{
    ColorScheme, Environment, NetworkPolicy, Plan, Step, Target, UiTreeMode, WaitCondition,
    valid_id,
};
use rupicola_protocol::UiAction;
use std::collections::HashSet;

const MAX_STEPS: usize = 64;
const CLIP_SAMPLES: std::ops::RangeInclusive<usize> = 2..=60;
const CLIP_INTERVAL_MS: std::ops::RangeInclusive<u32> = 1..=1000;
const MAX_CLIP_SPAN_MS: usize = 10_000;
const MAX_TOTAL_CLIP_MS: usize = 30_000;
const DEFAULT_CHECKPOINT: &str = "home";
const CSS_ENGINE: &str = "css=";
const CHORD_SEPARATOR: char = '+';
const MAX_WAIT_MS: u32 = 10_000;
const MAX_TOTAL_WAIT_MS: u64 = 30_000;
const THEME_LIGHT: &str = "light";
const THEME_DARK: &str = "dark";

/// Error produced when steps or a scenario cell cannot be turned into a [`Plan`].
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct PlanError(pub String);

impl PlanError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

fn selector_target(selector: &str) -> Result<String, PlanError> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(PlanError::new("step selector must not be empty"));
    }
    Ok(format!("{CSS_ENGINE}{trimmed}"))
}

fn optional_target(selector: Option<&str>) -> Result<Option<String>, PlanError> {
    selector.map(selector_target).transpose()
}

fn action_steps(action: &CaptureAction) -> Result<Vec<Step>, PlanError> {
    match action {
        CaptureAction::Click { selector } => Ok(vec![Step::Action {
            action: UiAction::Click {
                target_id: selector_target(selector)?,
            },
        }]),
        CaptureAction::Hover { selector } => Ok(vec![Step::Action {
            action: UiAction::Hover {
                target_id: selector_target(selector)?,
            },
        }]),
        CaptureAction::Fill { selector, value } => Ok(vec![Step::Action {
            action: UiAction::TypeText {
                target_id: selector_target(selector)?,
                text: value.clone(),
            },
        }]),
        CaptureAction::Press { selector, key } => {
            let mut parts = key
                .split(CHORD_SEPARATOR)
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let key = parts.pop().unwrap_or_default();
            if key.trim().is_empty() {
                return Err(PlanError::new("key press must not be empty"));
            }
            Ok(vec![
                Step::Action {
                    action: UiAction::Focus {
                        target_id: selector_target(selector)?,
                    },
                },
                Step::Action {
                    action: UiAction::PressKey {
                        key,
                        modifiers: parts,
                    },
                },
            ])
        }
        CaptureAction::Ready { selector } => Ok(vec![Step::Wait {
            condition: WaitCondition::Visible {
                target_id: selector_target(selector)?,
            },
        }]),
        CaptureAction::Scroll { selector, dx, dy } => Ok(vec![Step::Action {
            action: UiAction::Scroll {
                target_id: optional_target(selector.as_deref())?,
                dx: f64::from(*dx),
                dy: f64::from(*dy),
            },
        }]),
        CaptureAction::ScrollIntoView { selector } => Ok(vec![Step::Action {
            action: UiAction::ScrollIntoView {
                target_id: selector_target(selector)?,
            },
        }]),
        CaptureAction::ScrollTo { selector, x, y } => Ok(vec![Step::Action {
            action: UiAction::ScrollTo {
                target_id: optional_target(selector.as_deref())?,
                x: f64::from(*x),
                y: f64::from(*y),
            },
        }]),
        CaptureAction::ScrollToEnd { selector } => Ok(vec![Step::Action {
            action: UiAction::ScrollToEnd {
                target_id: optional_target(selector.as_deref())?,
            },
        }]),
        CaptureAction::Wait { ms } => {
            if *ms > MAX_WAIT_MS {
                return Err(PlanError::new(format!(
                    "a wait takes at most {MAX_WAIT_MS} ms"
                )));
            }
            Ok(vec![Step::Wait {
                condition: WaitCondition::Delay { ms: u64::from(*ms) },
            }])
        }
    }
}

fn step_into_plan(step: &CaptureStep) -> Result<(Vec<Step>, Option<u32>), PlanError> {
    match step {
        CaptureStep::Click { selector } => action_steps(&CaptureAction::Click {
            selector: selector.clone(),
        })
        .map(|s| (s, None)),
        CaptureStep::Hover { selector } => action_steps(&CaptureAction::Hover {
            selector: selector.clone(),
        })
        .map(|s| (s, None)),
        CaptureStep::Fill { selector, value } => action_steps(&CaptureAction::Fill {
            selector: selector.clone(),
            value: value.clone(),
        })
        .map(|s| (s, None)),
        CaptureStep::Press { selector, key } => action_steps(&CaptureAction::Press {
            selector: selector.clone(),
            key: key.clone(),
        })
        .map(|s| (s, None)),
        CaptureStep::Ready { selector } => action_steps(&CaptureAction::Ready {
            selector: selector.clone(),
        })
        .map(|s| (s, None)),
        CaptureStep::Scroll { selector, dx, dy } => action_steps(&CaptureAction::Scroll {
            selector: selector.clone(),
            dx: *dx,
            dy: *dy,
        })
        .map(|s| (s, None)),
        CaptureStep::ScrollIntoView { selector } => action_steps(&CaptureAction::ScrollIntoView {
            selector: selector.clone(),
        })
        .map(|s| (s, None)),
        CaptureStep::ScrollTo { selector, x, y } => action_steps(&CaptureAction::ScrollTo {
            selector: selector.clone(),
            x: *x,
            y: *y,
        })
        .map(|s| (s, None)),
        CaptureStep::ScrollToEnd { selector } => action_steps(&CaptureAction::ScrollToEnd {
            selector: selector.clone(),
        })
        .map(|s| (s, None)),
        CaptureStep::Wait { ms } => {
            action_steps(&CaptureAction::Wait { ms: *ms }).map(|s| (s, None))
        }
        CaptureStep::Checkpoint { id, full_page } => Ok((
            vec![Step::Checkpoint {
                id: id.clone(),
                full_page: *full_page,
            }],
            None,
        )),
        CaptureStep::Clip {
            id,
            trigger,
            samples,
            interval_ms,
        } => {
            let span = u64::try_from(samples.saturating_sub(1)).unwrap_or(u64::MAX)
                * u64::from(*interval_ms);
            let mut steps = vec![Step::StartRecording];
            steps.extend(action_steps(trigger)?);
            steps.push(Step::Wait {
                condition: WaitCondition::Delay { ms: span },
            });
            steps.push(Step::StopRecording { id: id.clone() });
            Ok((steps, Some(*interval_ms)))
        }
    }
}

/// Validates and converts a sequence of [`CaptureStep`]s into protocol [`Step`]s and an
/// optional minimum clip frame interval in milliseconds.
///
/// # Errors
///
/// Returns [`PlanError`] when any step, id, wait, or clip duration is out of bounds.
pub fn build_steps(steps: &[CaptureStep]) -> Result<(Vec<Step>, Option<u32>), PlanError> {
    let default_steps;
    let raw_steps = if steps.is_empty() {
        default_steps = [CaptureStep::Checkpoint {
            id: DEFAULT_CHECKPOINT.to_owned(),
            full_page: false,
        }];
        &default_steps[..]
    } else {
        steps
    };
    if raw_steps.len() > MAX_STEPS {
        return Err(PlanError::new(format!(
            "a capture takes at most {MAX_STEPS} steps"
        )));
    }
    let mut ids = HashSet::new();
    let mut checkpoints_or_clips = 0usize;
    let mut plan_steps = Vec::new();
    let mut min_interval: Option<u32> = None;
    let mut total_clip_ms = 0usize;
    let mut total_wait_ms = 0u64;

    for step in raw_steps {
        match step {
            CaptureStep::Wait { ms } => {
                total_wait_ms = total_wait_ms.saturating_add(u64::from(*ms));
                if total_wait_ms > MAX_TOTAL_WAIT_MS {
                    return Err(PlanError::new(format!(
                        "total wait time exceeds {MAX_TOTAL_WAIT_MS} ms"
                    )));
                }
            }
            CaptureStep::Checkpoint { id, .. } => {
                if !valid_id(id) || !ids.insert(id.to_ascii_lowercase()) {
                    return Err(PlanError::new(format!(
                        "invalid or duplicate checkpoint id `{id}`"
                    )));
                }
                checkpoints_or_clips += 1;
            }
            CaptureStep::Clip {
                id,
                samples,
                interval_ms,
                ..
            } => {
                if !valid_id(id) || !ids.insert(id.to_ascii_lowercase()) {
                    return Err(PlanError::new(format!(
                        "invalid or duplicate clip id `{id}`"
                    )));
                }
                let clip_ms = samples
                    .saturating_sub(1)
                    .saturating_mul(usize::try_from(*interval_ms).unwrap_or(usize::MAX));
                if !CLIP_SAMPLES.contains(samples)
                    || !CLIP_INTERVAL_MS.contains(interval_ms)
                    || clip_ms > MAX_CLIP_SPAN_MS
                {
                    return Err(PlanError::new(format!(
                        "clip `{id}` needs {}..={} samples at {}..={} ms (max {MAX_CLIP_SPAN_MS} ms)",
                        CLIP_SAMPLES.start(),
                        CLIP_SAMPLES.end(),
                        CLIP_INTERVAL_MS.start(),
                        CLIP_INTERVAL_MS.end(),
                    )));
                }
                total_clip_ms = total_clip_ms.saturating_add(clip_ms);
                if total_clip_ms > MAX_TOTAL_CLIP_MS {
                    return Err(PlanError::new(format!(
                        "total clip duration exceeds {MAX_TOTAL_CLIP_MS} ms"
                    )));
                }
                checkpoints_or_clips += 1;
            }
            _ => {}
        }
        let (converted, interval) = step_into_plan(step)?;
        plan_steps.extend(converted);
        if let Some(ms) = interval {
            min_interval = Some(min_interval.map_or(ms, |cur| cur.min(ms)));
        }
    }
    if checkpoints_or_clips == 0 {
        return Err(PlanError::new(
            "provide at least one checkpoint or clip step",
        ));
    }
    Ok((plan_steps, min_interval))
}

/// Validates and converts theme setup [`CaptureStep`]s (actions only, no checkpoints or clips).
///
/// # Errors
///
/// Returns [`PlanError`] for a checkpoint or clip, or an invalid action.
pub fn build_setup_steps(steps: &[CaptureStep]) -> Result<Vec<Step>, PlanError> {
    if steps.len() > MAX_STEPS {
        return Err(PlanError::new(format!(
            "a theme's setup takes at most {MAX_STEPS} steps"
        )));
    }
    let mut out = Vec::new();
    for step in steps {
        if matches!(
            step,
            CaptureStep::Checkpoint { .. } | CaptureStep::Clip { .. }
        ) {
            return Err(PlanError::new(
                "a theme's setup takes actions only, not checkpoints or clips",
            ));
        }
        let (converted, _) = step_into_plan(step)?;
        out.extend(converted);
    }
    Ok(out)
}

fn color_scheme_for_theme(theme: &str) -> Option<ColorScheme> {
    if theme.eq_ignore_ascii_case(THEME_LIGHT) {
        Some(ColorScheme::Light)
    } else if theme.eq_ignore_ascii_case(THEME_DARK) {
        Some(ColorScheme::Dark)
    } else {
        None
    }
}

fn has_checks(checks: &Checks) -> bool {
    checks.no_horizontal_overflow
        || checks.min_target_size.is_some()
        || !checks.no_overlap.is_empty()
        || checks.accessible_names
        || checks.contrast.is_some()
}

/// Builds a [`Plan`] for one `(viewport, theme)` cell of `scenario` against `base_url`.
///
/// A theme with non-empty setup steps runs those steps right after any leading `Ready` steps
/// and before the remaining scenario steps; a theme without setup steps maps `"light"` / `"dark"`
/// to `Environment.color_scheme`.
///
/// # Errors
///
/// Returns [`PlanError`] if any scenario or theme step is invalid.
pub fn scenario_cell_plan(
    scenario: &Scenario,
    base_url: &str,
    viewport: Viewport,
    theme: Option<&str>,
) -> Result<Plan, PlanError> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), scenario.url);

    let mut env = Environment::new([viewport.width, viewport.height]);
    env.scale = f64::from(viewport.scale);
    env.reduced_motion = scenario.environment.reduced_motion;
    env.warmup_ms = 0;
    env.locale = scenario
        .environment
        .locale
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned);
    env.network_policy = NetworkPolicy::Unrestricted;
    env.ui_tree = if has_checks(&scenario.checks) {
        UiTreeMode::Full
    } else {
        UiTreeMode::Off
    };

    let mut theme_plan_steps = Vec::new();
    if let Some(theme_name) = theme {
        let custom_steps = scenario
            .themes
            .get(theme_name)
            .map(|setup| setup.steps.as_slice())
            .filter(|steps| !steps.is_empty());
        if let Some(steps) = custom_steps {
            theme_plan_steps = build_setup_steps(steps)?;
        } else {
            env.color_scheme = color_scheme_for_theme(theme_name);
        }
    }

    let leading_ready = scenario
        .steps
        .iter()
        .take_while(|step| matches!(step, CaptureStep::Ready { .. }))
        .count();

    let (scenario_plan_steps, min_interval) = build_steps(&scenario.steps)?;
    env.frame_interval_ms = min_interval.map(u64::from);

    let steps = if theme_plan_steps.is_empty() {
        scenario_plan_steps
    } else {
        let mut combined = Vec::with_capacity(scenario_plan_steps.len() + theme_plan_steps.len());
        combined.extend_from_slice(&scenario_plan_steps[..leading_ready]);
        combined.extend(theme_plan_steps);
        combined.extend_from_slice(&scenario_plan_steps[leading_ready..]);
        combined
    };

    Ok(Plan {
        target: Target {
            command: url,
            args: Vec::new(),
        },
        environment: env,
        steps,
        allow_degraded: false,
    })
}

#[cfg(test)]
mod tests;
