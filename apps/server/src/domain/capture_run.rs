//! Validation and translation of [`RunCapture`] requests into declarative
//! `rupicola_capture::session::Plan` plans.

use super::{ApiError, capture::check_scenario};
use rupicola_api::{CaptureAction, CaptureStep, RunCapture};
use rupicola_capture::session::{
    ColorScheme, Environment, Plan, Step, Target, WaitCondition, valid_id,
};
use rupicola_protocol::UiAction;
use std::collections::HashSet;

const DEFAULT_WIDTH: u32 = 1440;
const DEFAULT_HEIGHT: u32 = 900;
const DEFAULT_SCALE: f64 = 1.0;
const MAX_URL_CHARS: usize = 2048;
const MAX_TIMEOUT_MS: u32 = 30_000;
const MAX_VIEWPORT_SIDE: u32 = 4096;
const MIN_SCALE: f64 = 0.5;
const MAX_SCALE: f64 = 3.0;
const MAX_DEVICE_PIXELS: f64 = 8_300_000.0;
const MAX_STEPS: usize = 64;
const CLIP_SAMPLES: std::ops::RangeInclusive<usize> = 2..=60;
const CLIP_INTERVAL_MS: std::ops::RangeInclusive<u32> = 1..=1000;
const MAX_CLIP_SPAN_MS: usize = 10_000;
const DEFAULT_CHECKPOINT: &str = "home";
const CSS_ENGINE: &str = "css=";
const CHORD_SEPARATOR: char = '+';

fn selector_target(selector: &str) -> Result<String, ApiError> {
    let trimmed = selector.trim();
    if trimmed.is_empty() {
        return Err(ApiError::invalid("step selector must not be empty"));
    }
    Ok(format!("{CSS_ENGINE}{trimmed}"))
}

fn action_steps(action: &CaptureAction) -> Result<Vec<Step>, ApiError> {
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
                return Err(ApiError::invalid("key press must not be empty"));
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
    }
}

fn step_into_plan(step: &CaptureStep) -> Result<(Vec<Step>, Option<u32>), ApiError> {
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
        CaptureStep::Checkpoint { id } => Ok((vec![Step::Checkpoint { id: id.clone() }], None)),
        CaptureStep::Clip {
            id,
            trigger,
            samples,
            interval_ms,
        } => {
            let span = u64::try_from(*samples).unwrap_or(u64::MAX) * u64::from(*interval_ms);
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

fn validate_url(raw: &str) -> Result<String, ApiError> {
    let url = raw.trim();
    if url.is_empty()
        || url.len() > MAX_URL_CHARS
        || url.chars().any(char::is_control)
        || url.contains(char::is_whitespace)
    {
        return Err(ApiError::invalid("capture URL must be a valid HTTP(S) URL"));
    }
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| ApiError::invalid("capture URL must use http:// or https://"))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return Err(ApiError::invalid("capture URL must include a host"));
    }
    Ok(url.to_owned())
}

fn parse_color_scheme(value: Option<&str>) -> Result<Option<ColorScheme>, ApiError> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) if v.eq_ignore_ascii_case("light") => Ok(Some(ColorScheme::Light)),
        Some(v) if v.eq_ignore_ascii_case("dark") => Ok(Some(ColorScheme::Dark)),
        Some(other) => Err(ApiError::invalid(format!(
            "unsupported color_scheme `{other}`: use `light` or `dark`"
        ))),
    }
}

fn build_environment(
    form: &RunCapture,
    frame_interval_ms: Option<u32>,
) -> Result<Environment, ApiError> {
    if form
        .timeout_ms
        .is_some_and(|n| !(1..=MAX_TIMEOUT_MS).contains(&n))
    {
        return Err(ApiError::invalid(format!(
            "timeout_ms must be in 1..={MAX_TIMEOUT_MS}"
        )));
    }
    let width = form.environment.width.unwrap_or(DEFAULT_WIDTH);
    let height = form.environment.height.unwrap_or(DEFAULT_HEIGHT);
    let scale = form.environment.scale.unwrap_or(DEFAULT_SCALE);
    if !(1..=MAX_VIEWPORT_SIDE).contains(&width)
        || !(1..=MAX_VIEWPORT_SIDE).contains(&height)
        || !(MIN_SCALE..=MAX_SCALE).contains(&scale)
        || !scale.is_finite()
        || (f64::from(width) * scale).ceil() * (f64::from(height) * scale).ceil()
            > MAX_DEVICE_PIXELS
    {
        return Err(ApiError::invalid(format!(
            "invalid viewport or scale (sides 1..={MAX_VIEWPORT_SIDE}, scale {MIN_SCALE}–{MAX_SCALE})"
        )));
    }
    let mut env = Environment::new([width, height]);
    env.scale = scale;
    env.color_scheme = parse_color_scheme(form.environment.color_scheme.as_deref())?;
    env.reduced_motion = form.environment.reduced_motion;
    env.warmup_ms = 0;
    env.locale = form
        .environment
        .locale
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned);
    env.timeout_ms = form.timeout_ms.map(u64::from);
    env.frame_interval_ms = frame_interval_ms.map(u64::from);
    Ok(env)
}

fn build_steps(steps: &[CaptureStep]) -> Result<(Vec<Step>, Option<u32>), ApiError> {
    let default_steps;
    let raw_steps = if steps.is_empty() {
        default_steps = [CaptureStep::Checkpoint {
            id: DEFAULT_CHECKPOINT.to_owned(),
        }];
        &default_steps[..]
    } else {
        steps
    };
    if raw_steps.len() > MAX_STEPS {
        return Err(ApiError::invalid(format!(
            "a capture takes at most {MAX_STEPS} steps"
        )));
    }
    let mut ids = HashSet::new();
    let mut checkpoints_or_clips = 0usize;
    let mut plan_steps = Vec::new();
    let mut min_interval: Option<u32> = None;

    for step in raw_steps {
        match step {
            CaptureStep::Checkpoint { id } => {
                if !valid_id(id) || !ids.insert(id.to_ascii_lowercase()) {
                    return Err(ApiError::invalid(format!(
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
                    return Err(ApiError::invalid(format!(
                        "invalid or duplicate clip id `{id}`"
                    )));
                }
                if !CLIP_SAMPLES.contains(samples)
                    || !CLIP_INTERVAL_MS.contains(interval_ms)
                    || (*samples - 1) * usize::try_from(*interval_ms).unwrap_or(usize::MAX)
                        > MAX_CLIP_SPAN_MS
                {
                    return Err(ApiError::invalid(format!(
                        "clip `{id}` needs {}..={} samples at {}..={} ms (max {MAX_CLIP_SPAN_MS} ms)",
                        CLIP_SAMPLES.start(),
                        CLIP_SAMPLES.end(),
                        CLIP_INTERVAL_MS.start(),
                        CLIP_INTERVAL_MS.end(),
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
        return Err(ApiError::invalid(
            "provide at least one checkpoint or clip step",
        ));
    }
    Ok((plan_steps, min_interval))
}

/// Validates `form` and turns it into a declarative capture [`Plan`].
///
/// # Errors
///
/// [`ApiError::Invalid`] when the URL, scenario, viewport or steps are out of bounds.
pub fn to_plan(form: &RunCapture) -> Result<Plan, ApiError> {
    check_scenario(&form.scenario)?;
    let url = validate_url(&form.url)?;
    let (steps, min_interval) = build_steps(&form.steps)?;
    let environment = build_environment(form, min_interval)?;

    Ok(Plan {
        target: Target {
            command: url,
            args: Vec::new(),
        },
        environment,
        steps,
        allow_degraded: form.allow_degraded,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::Environment as ApiEnvironment;

    #[test]
    fn default_run_builds_a_single_home_checkpoint_plan() {
        let req = RunCapture {
            scenario: "Home".to_owned(),
            url: "http://127.0.0.1:4731/".to_owned(),
            revision: None,
            environment: ApiEnvironment::default(),
            timeout_ms: None,
            steps: Vec::new(),
            allow_degraded: false,
        };
        let plan = to_plan(&req).unwrap();
        assert_eq!(plan.target.command, "http://127.0.0.1:4731/");
        assert_eq!(plan.environment.viewport, [DEFAULT_WIDTH, DEFAULT_HEIGHT]);
        assert_eq!(
            plan.steps,
            vec![Step::Checkpoint {
                id: "home".to_owned()
            }]
        );
    }

    #[test]
    fn rejects_non_http_urls_and_empty_targets() {
        let req = RunCapture {
            scenario: "Bad".to_owned(),
            url: "file:///etc/passwd".to_owned(),
            revision: None,
            environment: ApiEnvironment::default(),
            timeout_ms: None,
            steps: Vec::new(),
            allow_degraded: false,
        };
        assert!(matches!(to_plan(&req), Err(ApiError::Invalid(_))));
    }
}
