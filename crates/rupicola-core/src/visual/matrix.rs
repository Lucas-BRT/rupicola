//! Planning and running a capture matrix of external commands.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Shell exit codes reported for capture commands that could not start or ran
/// too long, as `sh` and `timeout(1)` do.
const EXIT_NOT_RUNNABLE: i32 = 127;
const EXIT_TIMED_OUT: i32 = 124;
/// How often a running capture command is polled.
const COMMAND_POLL: std::time::Duration = std::time::Duration::from_millis(50);

#[derive(Debug, Serialize)]
pub struct MatrixEntry {
    pub name: String,
    pub env: Vec<(String, String)>,
    pub exit: Option<i32>,
    pub png: Option<PathBuf>,
    pub ok: bool,
}

pub struct MatrixSpec {
    pub states: Vec<String>,
    pub themes: Vec<String>,
    pub scales: Vec<String>,
    pub text_scales: Vec<String>,
    pub sizes: Vec<String>,
    pub locales: Vec<String>,
}

#[must_use]
pub fn matrix_plan(spec: &MatrixSpec, out: &Path) -> Vec<MatrixEntry> {
    let mut entries = Vec::new();
    for state in &spec.states {
        for theme in &spec.themes {
            for scale in &spec.scales {
                for text in &spec.text_scales {
                    for size in &spec.sizes {
                        for locale in &spec.locales {
                            let name =
                                format!("{state}__{theme}__x{scale}__t{text}__{size}__{locale}");
                            let (w, h) = size.split_once('x').unwrap_or((size.as_str(), "0"));
                            let png = out.join(format!("{name}.png"));
                            let env = vec![
                                ("RUPICOLA_OUT".to_owned(), png.display().to_string()),
                                ("RUPICOLA_STATE".to_owned(), state.clone()),
                                ("RUPICOLA_THEME".to_owned(), theme.clone()),
                                ("RUPICOLA_SCALE".to_owned(), scale.clone()),
                                ("RUPICOLA_TEXT_SCALE".to_owned(), text.clone()),
                                ("RUPICOLA_SIZE".to_owned(), size.clone()),
                                ("RUPICOLA_WIDTH".to_owned(), w.to_owned()),
                                ("RUPICOLA_HEIGHT".to_owned(), h.to_owned()),
                                ("RUPICOLA_LOCALE".to_owned(), locale.clone()),
                            ];
                            entries.push(MatrixEntry {
                                name,
                                env,
                                exit: None,
                                png: Some(png),
                                ok: false,
                            });
                        }
                    }
                }
            }
        }
    }
    entries
}

pub fn matrix_run(entries: &mut [MatrixEntry], cmd: &str, timeout: std::time::Duration) {
    for entry in entries.iter_mut() {
        let mut command = std::process::Command::new("sh");
        command
            .args(["-c", cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        for (k, v) in &entry.env {
            command.env(k, v);
        }
        let Ok(mut child) = command.spawn() else {
            entry.exit = Some(EXIT_NOT_RUNNABLE);
            continue;
        };
        let start = std::time::Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s.code(),
                Ok(None) if start.elapsed() > timeout => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Some(EXIT_TIMED_OUT);
                }
                Ok(None) => std::thread::sleep(COMMAND_POLL),
                Err(_) => break None,
            }
        };
        entry.exit = status;
        entry.ok = status == Some(0) && entry.png.as_ref().is_some_and(|p| p.is_file());
    }
}
