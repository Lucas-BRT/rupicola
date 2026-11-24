//! What every command that reads or writes a backend resource shares: which backend, which
//! project, how to print the answer and where a request body comes from.

use crate::{
    EXIT_CLEAN, Result, json, service,
    session::{Remote, Server},
};
use anyhow::Context;
use clap::builder::{PossibleValuesParser, TypedValueParser};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    io::{self, Read},
    path::PathBuf,
    str::FromStr,
};

/// The `--file` value that stands for standard input.
const STDIN: &str = "-";

/// The backend a command talks to and how it prints the answer.
#[derive(clap::Args)]
pub struct Client {
    #[command(flatten)]
    server: Server,
    /// Print the answer as JSON.
    #[arg(long)]
    json: bool,
}

impl Client {
    /// A connection to the backend, using the stored or environment token.
    ///
    /// # Errors
    ///
    /// Fails when there is no backend or token.
    pub fn remote(&self) -> Result<Remote> {
        self.server.connect()
    }

    /// Prints `value` as JSON when asked to, else with `human`; always a clean exit.
    ///
    /// # Errors
    ///
    /// Fails when the JSON cannot be written.
    pub fn show<T: Serialize>(&self, value: &T, human: impl FnOnce(&T)) -> Result<u8> {
        if self.json {
            json(value)?;
        } else {
            human(value);
        }
        Ok(EXIT_CLEAN)
    }
}

/// A [`Client`] for commands that act inside one project.
#[derive(clap::Args)]
pub struct Scoped {
    #[command(flatten)]
    client: Client,
    /// Project to act in; defaults to the one this checkout is linked to, or Git detects.
    #[arg(long)]
    project: Option<String>,
}

impl Scoped {
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// The project: the flag, else the checkout's link, else the one Git detects.
    ///
    /// # Errors
    ///
    /// Fails outside a Git checkout when no project is named.
    pub fn project(&self) -> Result<String> {
        match &self.project {
            Some(project) => Ok(project.clone()),
            None => Ok(service::detect_project()?.id),
        }
    }
}

/// A request body given as a JSON file, or `-` for standard input.
#[derive(clap::Args)]
pub struct Body {
    /// JSON file holding the request body, or `-` for standard input.
    #[arg(long, value_name = "PATH")]
    file: PathBuf,
}

impl Body {
    /// The body, parsed as `T`.
    ///
    /// # Errors
    ///
    /// Fails when the file cannot be read or does not hold a valid `T`.
    pub fn read<T: DeserializeOwned>(&self) -> Result<T> {
        let text = if self.file.as_os_str() == STDIN {
            let mut text = String::new();
            io::stdin()
                .read_to_string(&mut text)
                .context("Read the body from stdin")?;
            text
        } else {
            std::fs::read_to_string(&self.file)
                .with_context(|| format!("Read {}", self.file.display()))?
        };
        serde_json::from_str(&text).context("The body is not the JSON this command expects")
    }
}

/// A value parser for an enumeration given by its names, so `--help` and a wrong value list
/// the choices.
pub fn choice<T>(names: &'static [&'static str]) -> impl TypedValueParser<Value = T>
where
    T: FromStr + Clone + Send + Sync + 'static,
{
    PossibleValuesParser::new(names).try_map(|name| {
        name.parse::<T>()
            .map_err(|_| format!("`{name}` is not one of {}", names.join(", ")))
    })
}

/// Prints one line per row, or `empty` when there are none.
pub fn rows<T>(items: &[T], empty: &str, line: impl Fn(&T) -> String) {
    if items.is_empty() {
        println!("{empty}");
    }
    for item in items {
        println!("{}", line(item));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use rupicola_api::{NewComment, Severity};

    const COMMENT_JSON: &str = r#"{"text":"looks right"}"#;

    #[derive(Parser)]
    struct Probe {
        #[arg(long, value_parser = choice::<Severity>(Severity::NAMES))]
        severity: Severity,
    }

    #[test]
    fn a_choice_takes_a_listed_name_and_names_the_options_otherwise() {
        let known = Severity::NAMES[0];
        let probe = Probe::try_parse_from(["probe", "--severity", known]).unwrap();
        assert_eq!(probe.severity.as_str(), known);
        let error = Probe::try_parse_from(["probe", "--severity", "nonsense"])
            .err()
            .unwrap()
            .to_string();
        for name in Severity::NAMES {
            assert!(error.contains(name), "{error}");
        }
    }

    #[test]
    fn a_body_is_read_from_a_file_and_must_match_its_type() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("body.json");
        std::fs::write(&path, COMMENT_JSON).unwrap();
        let body = Body { file: path.clone() };
        assert_eq!(body.read::<NewComment>().unwrap().text, "looks right");
        std::fs::write(&path, "[]").unwrap();
        assert!(body.read::<NewComment>().is_err());
        assert!(
            Body {
                file: dir.path().join("missing.json")
            }
            .read::<NewComment>()
            .is_err()
        );
    }
}
