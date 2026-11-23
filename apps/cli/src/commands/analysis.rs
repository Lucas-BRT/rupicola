//! Source and skill analysis: discover, scan, check, rules and modules.

use crate::{
    EXIT_CLEAN, EXIT_ERROR, EXIT_FINDINGS, EXIT_UNSUPPORTED, FRAMEWORKS, Result, TOPICS, check,
    discover, json, scan,
};
use std::path::PathBuf;

#[derive(clap::Args)]
pub struct DiscoverArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    json: bool,
}

impl DiscoverArgs {
    pub(super) fn run(self) -> Result<u8> {
        let DiscoverArgs {
            path,
            json: output_json,
        } = self;
        let result = discover::discover(&path)?;
        if output_json {
            json(&result)?;
        } else {
            for package in &result.packages {
                println!(
                    "{} [{}] {}",
                    package.name,
                    package
                        .frameworks
                        .iter()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", "),
                    package.manifest.display()
                );
            }
            for d in &result.diagnostics {
                eprintln!("{}: {}", d.path.display(), d.message);
            }
            println!("{}", result.configuration);
        }
        Ok(u8::from(!result.diagnostics.is_empty()))
    }
}

#[derive(clap::Args)]
pub struct ScanArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long, value_parser = clap::builder::PossibleValuesParser::new(FRAMEWORKS))]
    framework: Option<String>,
    #[arg(long)]
    include_tests: bool,
    #[arg(long)]
    json: bool,
}

impl ScanArgs {
    pub(super) fn run(self) -> Result<u8> {
        let ScanArgs {
            path,
            framework,
            include_tests,
            json: output_json,
        } = self;
        let result = scan::scan(&path, framework.as_deref(), include_tests)?;
        if output_json {
            json(&result)?;
        } else {
            println!(
                "{} Rust files; {} parsed; {} unsupported; {} candidates",
                result.rust_files,
                result.parsed_files,
                result.unsupported_files.len(),
                result.findings.len()
            );
            for f in &result.findings {
                println!(
                    "{}:{}:{} [{}] {}",
                    f.file.display(),
                    f.line,
                    f.column,
                    f.rule,
                    f.message
                );
            }
            for d in &result.diagnostics {
                eprintln!("{}: {}", d.path.display(), d.message);
            }
            for limit in &result.limitations {
                println!("{limit}");
            }
        }
        Ok(if !result.diagnostics.is_empty() {
            EXIT_ERROR
        } else if !result.findings.is_empty() {
            EXIT_FINDINGS
        } else if result.parsed_files == 0 || !result.unsupported_files.is_empty() {
            EXIT_UNSUPPORTED
        } else {
            EXIT_CLEAN
        })
    }
}

#[derive(clap::Args)]
pub struct CheckArgs {
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long)]
    json: bool,
}

impl CheckArgs {
    pub(super) fn run(self) -> Result<u8> {
        let CheckArgs {
            path,
            json: output_json,
        } = self;
        let result = check::check(&path)?;
        if output_json {
            json(&result)?;
        } else {
            println!(
                "{} Markdown files checked; {} issues",
                result.markdown_files,
                result.diagnostics.len()
            );
            for d in &result.diagnostics {
                eprintln!("{}: {}", d.path.display(), d.message);
            }
        }
        Ok(u8::from(!result.diagnostics.is_empty()))
    }
}

#[derive(clap::Args)]
pub struct RulesArgs {
    #[arg(long)]
    json: bool,
}

impl RulesArgs {
    pub(super) fn run(self) -> Result<u8> {
        let RulesArgs { json: output_json } = self;
        if output_json {
            json(&scan::RULES)?;
        } else {
            for (id, description) in scan::RULES {
                println!("{id}: {description}");
            }
        }
        Ok(EXIT_CLEAN)
    }
}

#[derive(clap::Args)]
pub struct ModulesArgs {
    #[arg(long)]
    json: bool,
}

impl ModulesArgs {
    pub(super) fn run(self) -> Result<u8> {
        let ModulesArgs { json: output_json } = self;
        let modules: Vec<_> = FRAMEWORKS
            .iter()
            .map(|id| format!("reference/frameworks/{id}.md"))
            .chain(TOPICS.iter().map(|id| format!("reference/ux/{id}.md")))
            .collect();
        if output_json {
            json(&modules)?;
        } else {
            for module in modules {
                println!("{module}");
            }
        }
        Ok(EXIT_CLEAN)
    }
}
