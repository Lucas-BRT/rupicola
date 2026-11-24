//! Validation and translation of [`RunCapture`] requests into declarative
//! `rupicola_capture::session::Plan` plans.

use super::{ApiError, capture::check_scenario};
use rupicola_api::{CaptureStep, RunCapture, plan};
use rupicola_capture::session::{ColorScheme, Environment, NetworkPolicy, Plan, Target};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub const DEFAULT_WIDTH: u32 = 1440;
pub const DEFAULT_HEIGHT: u32 = 900;
const DEFAULT_SCALE: f64 = 1.0;
const MAX_URL_CHARS: usize = 2048;
const HTTP_DEFAULT_PORT: u16 = 80;
const HTTPS_DEFAULT_PORT: u16 = 443;
const MAX_TIMEOUT_MS: u32 = 30_000;
const MAX_VIEWPORT_SIDE: u32 = 4096;
const MIN_SCALE: f64 = 0.5;
const MAX_SCALE: f64 = 3.0;
const MAX_DEVICE_PIXELS: f64 = 8_300_000.0;

/// Validated capture target URL with its extracted host and port for DNS/SSRF checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedTarget {
    pub url: String,
    pub host: String,
    pub port: u16,
}

const CGNAT_PREFIX: u8 = 100;
const CGNAT_MASK: u8 = 0b1100_0000;
const CGNAT_BASE: u8 = 64;

fn is_forbidden_v4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    let is_cgnat = octets[0] == CGNAT_PREFIX && (octets[1] & CGNAT_MASK) == CGNAT_BASE;
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        || is_cgnat
}

/// Whether `ip` is a loopback, private, link-local, unspecified, multicast, or metadata address.
pub fn is_forbidden_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_forbidden_v4(v4),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.to_ipv4_mapped().is_some_and(is_forbidden_v4)
        }
    }
}

fn split_authority(authority: &str, default_port: u16) -> Result<(String, u16), ApiError> {
    if authority.is_empty() {
        return Err(ApiError::invalid("capture URL must include a host"));
    }
    if authority.contains('@') {
        return Err(ApiError::invalid("capture URL must not contain userinfo"));
    }
    if authority.contains('\\') {
        return Err(ApiError::invalid("capture URL host is invalid"));
    }
    if let Some(rest) = authority.strip_prefix('[') {
        let Some(end) = rest.find(']') else {
            return Err(ApiError::invalid("invalid IPv6 host in capture URL"));
        };
        let host = &rest[..end];
        if host.parse::<Ipv6Addr>().is_err() {
            return Err(ApiError::invalid("invalid IPv6 address in capture URL"));
        }
        let after = &rest[end + 1..];
        let port = if after.is_empty() {
            default_port
        } else if let Some(port_str) = after.strip_prefix(':') {
            parse_port(port_str)?
        } else {
            return Err(ApiError::invalid("invalid port in capture URL"));
        };
        return Ok((host.to_owned(), port));
    }
    if authority.contains('[') || authority.contains(']') {
        return Err(ApiError::invalid("invalid host brackets in capture URL"));
    }
    if let Some((host, port_str)) = authority.split_once(':') {
        if host.is_empty() || port_str.contains(':') {
            return Err(ApiError::invalid("invalid host or port in capture URL"));
        }
        return Ok((host.to_owned(), parse_port(port_str)?));
    }
    Ok((authority.to_owned(), default_port))
}

fn parse_port(raw: &str) -> Result<u16, ApiError> {
    let port: u16 = raw
        .parse()
        .map_err(|_| ApiError::invalid("capture URL port must be a valid TCP port"))?;
    if port == 0 {
        return Err(ApiError::invalid("capture URL port must be non-zero"));
    }
    Ok(port)
}

fn validate_url(raw: &str, allow_local: bool) -> Result<ParsedTarget, ApiError> {
    let url = raw.trim();
    if url.is_empty()
        || url.len() > MAX_URL_CHARS
        || url.chars().any(char::is_control)
        || url.contains(char::is_whitespace)
    {
        return Err(ApiError::invalid("capture URL must be a valid HTTP(S) URL"));
    }
    let (rest, default_port) = if let Some(rest) = url.strip_prefix("http://") {
        (rest, HTTP_DEFAULT_PORT)
    } else if let Some(rest) = url.strip_prefix("https://") {
        (rest, HTTPS_DEFAULT_PORT)
    } else {
        return Err(ApiError::invalid(
            "capture URL must use http:// or https://",
        ));
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let (host, port) = split_authority(authority, default_port)?;
    if !allow_local {
        let lower = host.to_ascii_lowercase();
        if lower == "localhost" || lower.ends_with(".localhost") {
            return Err(ApiError::invalid(
                "capture target must not point to localhost",
            ));
        }
        if let Ok(ip) = host.parse::<IpAddr>()
            && is_forbidden_ip(ip)
        {
            return Err(ApiError::invalid(
                "capture target must not point to a local, private, or metadata address",
            ));
        }
    }
    Ok(ParsedTarget {
        url: url.to_owned(),
        host,
        port,
    })
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

/// Checks a scenario's steps as `capture_run` would: bounded, with distinct checkpoint and clip
/// ids and at least one of them.
///
/// # Errors
///
/// [`ApiError::Invalid`] when the steps would not make a valid run.
pub fn check_steps(steps: &[CaptureStep]) -> Result<(), ApiError> {
    plan::build_steps(steps)
        .map(|_| ())
        .map_err(|err| ApiError::invalid(err.to_string()))
}

/// Checks the steps that put a page in a theme: actions only, since a theme's setup captures
/// nothing.
///
/// # Errors
///
/// [`ApiError::Invalid`] for a checkpoint or clip, or an action that would not run.
pub fn check_setup_steps(steps: &[CaptureStep]) -> Result<(), ApiError> {
    plan::build_setup_steps(steps)
        .map(|_| ())
        .map_err(|err| ApiError::invalid(err.to_string()))
}

/// Validates `form` and turns it into a declarative capture [`Plan`] and [`ParsedTarget`].
///
/// # Errors
///
/// [`ApiError::Invalid`] when the URL, scenario, viewport or steps are out of bounds.
pub fn to_plan(form: &RunCapture, allow_local: bool) -> Result<(Plan, ParsedTarget), ApiError> {
    check_scenario(&form.scenario)?;
    let target = validate_url(&form.url, allow_local)?;
    let (steps, min_interval) =
        plan::build_steps(&form.steps).map_err(|err| ApiError::invalid(err.to_string()))?;
    let mut environment = build_environment(form, min_interval)?;
    environment.network_policy = if allow_local {
        NetworkPolicy::Unrestricted
    } else {
        NetworkPolicy::PublicOnly
    };

    Ok((
        Plan {
            target: Target {
                command: target.url.clone(),
                args: Vec::new(),
            },
            environment,
            steps,
            allow_degraded: form.allow_degraded,
        },
        target,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rupicola_api::{CaptureAction, Environment as ApiEnvironment};
    use rupicola_capture::session::{Step, WaitCondition};
    use rupicola_protocol::UiAction;

    const MAX_WAIT_MS: u32 = 10_000;

    fn req(url: &str) -> RunCapture {
        RunCapture {
            scenario: "Home".to_owned(),
            url: url.to_owned(),
            revision: None,
            environment: ApiEnvironment::default(),
            timeout_ms: None,
            steps: Vec::new(),
            allow_degraded: false,
        }
    }

    #[test]
    fn default_run_builds_a_single_home_checkpoint_plan() {
        let (plan, target) = to_plan(&req("https://example.com/"), false).unwrap();
        assert_eq!(plan.target.command, "https://example.com/");
        assert_eq!(target.host, "example.com");
        assert_eq!(target.port, HTTPS_DEFAULT_PORT);
        assert_eq!(plan.environment.viewport, [DEFAULT_WIDTH, DEFAULT_HEIGHT]);
        assert_eq!(
            plan.steps,
            vec![Step::Checkpoint {
                id: "home".to_owned(),
                full_page: false
            }]
        );
    }

    fn run_with(steps: Vec<CaptureStep>) -> Result<Vec<Step>, ApiError> {
        let mut form = req("https://example.com/");
        form.steps = steps;
        to_plan(&form, false).map(|(plan, _)| plan.steps)
    }

    fn checkpoint(id: &str, full_page: bool) -> CaptureStep {
        CaptureStep::Checkpoint {
            id: id.to_owned(),
            full_page,
        }
    }

    #[test]
    fn scroll_steps_become_protocol_actions_on_css_targets() {
        let steps = run_with(vec![
            CaptureStep::ScrollIntoView {
                selector: "#deep".to_owned(),
            },
            CaptureStep::Scroll {
                selector: None,
                dx: 0,
                dy: 300,
            },
            CaptureStep::ScrollTo {
                selector: Some(".list".to_owned()),
                x: 0,
                y: 120,
            },
            CaptureStep::ScrollToEnd { selector: None },
            checkpoint("end", true),
        ])
        .unwrap();
        let action = |action| Step::Action { action };
        assert_eq!(
            steps,
            vec![
                action(UiAction::ScrollIntoView {
                    target_id: "css=#deep".to_owned()
                }),
                action(UiAction::Scroll {
                    target_id: None,
                    dx: 0.0,
                    dy: 300.0
                }),
                action(UiAction::ScrollTo {
                    target_id: Some("css=.list".to_owned()),
                    x: 0.0,
                    y: 120.0
                }),
                action(UiAction::ScrollToEnd { target_id: None }),
                Step::Checkpoint {
                    id: "end".to_owned(),
                    full_page: true
                },
            ]
        );
    }

    #[test]
    fn a_wait_step_is_a_delay_and_is_bounded() {
        let steps = run_with(vec![
            CaptureStep::Wait { ms: 250 },
            checkpoint("home", false),
        ])
        .unwrap();
        assert_eq!(
            steps[0],
            Step::Wait {
                condition: WaitCondition::Delay { ms: 250 }
            }
        );
        let too_long = run_with(vec![
            CaptureStep::Wait {
                ms: MAX_WAIT_MS + 1,
            },
            checkpoint("home", false),
        ]);
        assert!(matches!(too_long, Err(ApiError::Invalid(_))));
        let wait = CaptureStep::Wait { ms: MAX_WAIT_MS };
        let too_much = run_with(vec![
            wait.clone(),
            wait.clone(),
            wait.clone(),
            wait,
            checkpoint("home", false),
        ]);
        assert!(matches!(too_much, Err(ApiError::Invalid(_))));
    }

    #[test]
    fn an_empty_scroll_selector_is_refused() {
        let result = run_with(vec![
            CaptureStep::ScrollIntoView {
                selector: "  ".to_owned(),
            },
            checkpoint("home", false),
        ]);
        assert!(matches!(result, Err(ApiError::Invalid(_))));
    }

    #[test]
    fn json_steps_default_full_page_off_and_selector_absent() {
        let steps: Vec<CaptureStep> = serde_json::from_str(
            r#"[{"kind":"scroll_to_end"},{"kind":"checkpoint","id":"a"},
                {"kind":"checkpoint","id":"b","full_page":true}]"#,
        )
        .unwrap();
        assert_eq!(steps[0], CaptureStep::ScrollToEnd { selector: None });
        assert_eq!(steps[1], checkpoint("a", false));
        assert_eq!(steps[2], checkpoint("b", true));
    }

    #[test]
    fn rejects_non_http_urls_and_userinfo() {
        assert!(matches!(
            to_plan(&req("file:///etc/passwd"), false),
            Err(ApiError::Invalid(_))
        ));
        assert!(matches!(
            to_plan(&req("http://user:pass@example.com"), false),
            Err(ApiError::Invalid(_))
        ));
        assert!(matches!(
            to_plan(&req("http://user:pass@example.com"), true),
            Err(ApiError::Invalid(_))
        ));
    }

    #[test]
    fn rejects_ssrf_targets_unless_local_captures_allowed() {
        assert!(matches!(
            to_plan(&req("http://169.254.169.254/latest/meta-data/"), false),
            Err(ApiError::Invalid(_))
        ));
        assert!(matches!(
            to_plan(&req("http://127.0.0.1:4731/"), false),
            Err(ApiError::Invalid(_))
        ));
        assert!(matches!(
            to_plan(&req("http://[::ffff:127.0.0.1]/"), false),
            Err(ApiError::Invalid(_))
        ));
        let (plan, _) = to_plan(&req("http://127.0.0.1:4731/"), true).unwrap();
        assert_eq!(plan.target.command, "http://127.0.0.1:4731/");
    }

    #[test]
    fn clip_delay_uses_samples_minus_one_and_caps_total_duration() {
        let mut form = req("https://example.com/");
        form.steps = vec![CaptureStep::Clip {
            id: "hover".to_owned(),
            trigger: CaptureAction::Hover {
                selector: "button".to_owned(),
            },
            samples: 11,
            interval_ms: 1000,
        }];
        let (plan, _) = to_plan(&form, false).unwrap();
        assert!(plan.steps.contains(&Step::Wait {
            condition: WaitCondition::Delay { ms: 10_000 }
        }));

        let clip = |id: &str| CaptureStep::Clip {
            id: id.to_owned(),
            trigger: CaptureAction::Hover {
                selector: "button".to_owned(),
            },
            samples: 11,
            interval_ms: 1000,
        };
        form.steps = vec![clip("a"), clip("b"), clip("c"), clip("d")];
        assert!(matches!(to_plan(&form, false), Err(ApiError::Invalid(_))));
    }
}
