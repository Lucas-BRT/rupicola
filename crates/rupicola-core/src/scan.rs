use crate::{Diagnostic, discover, files, read};
use proc_macro2::{Span, TokenStream, TokenTree};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
use syn::visit::{self, Visit};

pub const RULES: &[(&str, &str)] = &[
    (
        "transition-all",
        "Review broad CSS transitions; restrict properties and respect reduced motion.",
    ),
    (
        "gradient-text",
        "Review whether gradient text remains readable and follows product identity.",
    ),
    (
        "focus-outline",
        "Removing an outline needs an alternative visible keyboard focus indicator.",
    ),
    (
        "hardcoded-color",
        "A literal color in UI code bypasses theme roles; confirm it is a deliberate brand color or move it to a token.",
    ),
    (
        "blocking-ui",
        "Blocking IO or sleeping in a UI lifecycle function may stall interaction; verify execution context.",
    ),
];
#[derive(Debug, Serialize)]
pub struct Finding {
    pub rule: String,
    pub file: PathBuf,
    pub line: usize,
    pub column: usize,
    pub snippet: String,
    pub message: String,
    pub status: &'static str,
}
#[derive(Debug, Default, Serialize)]
pub struct Scan {
    pub schema_version: u8,
    pub rust_files: usize,
    pub parsed_files: usize,
    pub unsupported_files: Vec<PathBuf>,
    pub findings: Vec<Finding>,
    pub diagnostics: Vec<Diagnostic>,
    pub limitations: Vec<&'static str>,
}
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    ignore_rules: Vec<String>,
    ignore_paths: Vec<String>,
}

fn config(root: &Path) -> anyhow::Result<(Config, PathBuf)> {
    let start = if root.is_file() {
        root.parent().unwrap()
    } else {
        root
    };
    for dir in start.ancestors() {
        let path = dir.join(".rupicola/config.toml");
        if path.is_file() {
            let config: Config = toml::from_str(&read(&path)?)
                .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?;
            for id in &config.ignore_rules {
                if !RULES.iter().any(|(rule, _)| rule == id) {
                    anyhow::bail!("unknown ignored rule: {id}");
                }
            }
            for p in &config.ignore_paths {
                if p.is_empty()
                    || Path::new(p).is_absolute()
                    || Path::new(p)
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
                {
                    anyhow::bail!(
                        "ignore_paths entries must be nonempty relative paths without '..': {p}"
                    );
                }
            }
            return Ok((config, dir.to_owned()));
        }
    }
    Ok((Config::default(), start.to_owned()))
}
struct Inspector<'a> {
    source: &'a str,
    path: &'a Path,
    frameworks: &'a BTreeSet<String>,
    ignored: &'a [String],
    findings: Vec<Finding>,
    seen: BTreeSet<(String, usize)>,
    ui_depth: usize,
}
impl Inspector<'_> {
    fn hit(&mut self, id: &str, span: Span) {
        if self.ignored.iter().any(|r| r == id) {
            return;
        }
        let pos = span.start();
        // Only standalone comment directives; string literals cannot suppress findings.
        let previous = pos
            .line
            .checked_sub(2)
            .and_then(|i| self.source.lines().nth(i))
            .unwrap_or("")
            .trim();
        if let Some(value) = previous.strip_prefix("// rupicola-ignore") {
            let value = value.trim();
            if value.is_empty() || value.split(',').any(|s| s.trim() == id) {
                return;
            }
        }
        if !self.seen.insert((id.to_owned(), pos.line)) {
            return;
        }
        self.findings.push(Finding {
            rule: id.to_owned(),
            file: self.path.to_owned(),
            line: pos.line,
            column: pos.column + 1,
            snippet: self
                .source
                .lines()
                .nth(pos.line.saturating_sub(1))
                .unwrap_or("")
                .trim()
                .chars()
                .take(160)
                .collect(),
            message: RULES.iter().find(|(r, _)| *r == id).unwrap().1.to_owned(),
            status: "candidate: verify in context",
        });
    }
    fn classes(&mut self, literal: &proc_macro2::Literal) {
        let Ok(value) = syn::parse_str::<syn::LitStr>(&literal.to_string()) else {
            return;
        };
        for class in value.value().split_whitespace() {
            let base = class.rsplit(':').next().unwrap_or(class);
            match base {
                "transition-all" => self.hit("transition-all", literal.span()),
                "bg-clip-text" => self.hit("gradient-text", literal.span()),
                "outline-none" | "outline-hidden" => self.hit("focus-outline", literal.span()),
                _ => {}
            }
        }
    }
    fn markup(&mut self, stream: TokenStream, class_macro: bool) {
        let tokens: Vec<_> = stream.into_iter().collect();
        for (i, token) in tokens.iter().enumerate() {
            if class_macro && let TokenTree::Literal(literal) = token {
                self.classes(literal);
            }
            if let TokenTree::Ident(ident) = token {
                if (ident == "class" || ident == "className")
                    && let (Some(TokenTree::Punct(separator)), Some(TokenTree::Literal(value))) =
                        (tokens.get(i + 1), tokens.get(i + 2))
                    && matches!(separator.as_char(), '=' | ':')
                {
                    self.classes(value);
                }
                if ident == "class"
                    && matches!(tokens.get(i+1), Some(TokenTree::Punct(p)) if p.as_char() == '!')
                    && let Some(TokenTree::Group(g)) = tokens.get(i + 2)
                {
                    self.markup(g.stream(), true);
                }
            }
            if let TokenTree::Group(group) = token {
                self.markup(group.stream(), class_macro);
            }
        }
    }
}
impl<'ast> Visit<'ast> for Inspector<'_> {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let Some(name) = mac.path.segments.last().map(|s| s.ident.to_string()) else {
            return;
        };
        let applicable = match name.as_str() {
            "view" => self.frameworks.contains("topcoat") || self.frameworks.contains("leptos"),
            "attributes" | "class" => self.frameworks.contains("topcoat"),
            "html" => self.frameworks.contains("yew"),
            "rsx" => self.frameworks.contains("dioxus"),
            _ => false,
        };
        if applicable {
            self.markup(mac.tokens.clone(), name == "class");
        }
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let ui = matches!(
            node.sig.ident.to_string().as_str(),
            "view" | "render" | "update" | "ui"
        );
        self.ui_depth += usize::from(ui);
        visit::visit_item_fn(self, node);
        self.ui_depth -= usize::from(ui);
    }
    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let ui = matches!(
            node.sig.ident.to_string().as_str(),
            "view" | "render" | "update" | "ui"
        );
        self.ui_depth += usize::from(ui);
        visit::visit_impl_item_fn(self, node);
        self.ui_depth -= usize::from(ui);
    }
    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        if self.ui_depth > 0
            && let syn::Expr::Path(path) = &*node.func
        {
            let name = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            if matches!(
                name.as_str(),
                "std::thread::sleep"
                    | "thread::sleep"
                    | "std::fs::read"
                    | "std::fs::read_to_string"
                    | "fs::read"
                    | "fs::read_to_string"
            ) {
                use syn::spanned::Spanned;
                self.hit("blocking-ui", path.span());
            }
            // Color::from_rgb8(0x12, 0x34, 0x56) and similar: all-literal arguments.
            let segments: Vec<String> = path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if let [.., owner, ctor] = segments.as_slice() {
                let is_color_type = matches!(
                    owner.as_str(),
                    "Color" | "Color32" | "Rgba" | "Srgba" | "Hsla"
                );
                let is_ctor = matches!(
                    ctor.as_str(),
                    "from_rgb"
                        | "from_rgb8"
                        | "from_rgba"
                        | "from_rgba8"
                        | "from_gray"
                        | "from_rgba_unmultiplied"
                        | "from_rgba_premultiplied"
                );
                let literal = |e: &syn::Expr| match e {
                    syn::Expr::Lit(_) => true,
                    syn::Expr::Unary(u) => matches!(&*u.expr, syn::Expr::Lit(_)),
                    _ => false,
                };
                if is_color_type
                    && is_ctor
                    && !node.args.is_empty()
                    && node.args.iter().all(literal)
                {
                    use syn::spanned::Spanned;
                    self.hit("hardcoded-color", path.span());
                }
            }
        }
        visit::visit_expr_call(self, node);
    }
}

/// # Errors
///
/// Returns an error if the input is not valid Rust syntax.
pub fn source(
    source: &str,
    path: &Path,
    frameworks: &BTreeSet<String>,
    ignore: &[String],
) -> anyhow::Result<Vec<Finding>> {
    let file = syn::parse_file(source).map_err(|e| {
        anyhow::anyhow!(
            "Rust parse failed at {}:{}: {e}",
            e.span().start().line,
            e.span().start().column + 1
        )
    })?;
    let mut visitor = Inspector {
        source,
        path,
        frameworks,
        ignored: ignore,
        findings: Vec::new(),
        seen: BTreeSet::new(),
        ui_depth: 0,
    };
    visitor.visit_file(&file);
    Ok(visitor.findings)
}

/// # Errors
///
/// Returns an error if the root cannot be resolved, the scan configuration is invalid or unreadable, or files cannot be enumerated.
pub fn scan(root: &Path, explicit: Option<&str>, include_tests: bool) -> anyhow::Result<Scan> {
    let root = root.canonicalize()?;
    let (config, config_root) = config(&root)?;
    let mut result = Scan {
        schema_version: 1,
        limitations: vec![
            "Candidates only; no visual inspection, execution, profiling or accessibility conformance testing.",
            "No macro expansion, name resolution, cfg evaluation or interprocedural analysis. Recognized macro names are syntactic candidates, not proven framework symbols.",
            "Class rules inspect literal class attributes in recognized Rust web macros only. Native checks cover direct blocking calls in view/render/update functions.",
            "Slint, CSS, HTML, generated files and dynamic classes are not analysed. Native semantics, missing states and performance require manual review.",
        ],
        ..Default::default()
    };
    let mut cache = std::collections::BTreeMap::new();
    for path in files(&root, include_tests)? {
        if config.ignore_paths.iter().any(|p| {
            path.strip_prefix(&config_root)
                .is_ok_and(|rel| rel.starts_with(p))
        }) {
            continue;
        }
        let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
        if matches!(ext, "slint" | "css" | "html" | "scss") {
            result.unsupported_files.push(path);
            continue;
        }
        if ext != "rs" {
            continue;
        }
        result.rust_files += 1;
        let text = match read(&path) {
            Ok(text) => text,
            Err(error) => {
                result.diagnostics.push(Diagnostic {
                    path,
                    message: error.to_string(),
                });
                continue;
            }
        };
        if text.len() > 1_000_000 {
            result.diagnostics.push(Diagnostic {
                path,
                message: "source exceeds the 1 MB scan limit".into(),
            });
            continue;
        }
        let mut frameworks = BTreeSet::new();
        if let Some(id) = explicit {
            frameworks.insert(id.to_owned());
        } else if let Some(parent) = path.parent() {
            for dir in parent.ancestors() {
                let manifest = dir.join("Cargo.toml");
                if manifest.is_file() {
                    let discovered = cache
                        .entry(manifest.clone())
                        .or_insert_with(|| discover::package(&manifest));
                    match discovered {
                        Ok(Some(p)) => frameworks.clone_from(&p.frameworks),
                        Ok(None) => {}
                        Err(error) => result.diagnostics.push(Diagnostic {
                            path: manifest,
                            message: error.to_string(),
                        }),
                    }
                    break;
                }
            }
        }
        if frameworks.is_empty() {
            result.unsupported_files.push(path);
            continue;
        }
        match source(&text, &path, &frameworks, &config.ignore_rules) {
            Ok(hits) => {
                result.parsed_files += 1;
                result.findings.extend(hits);
            }
            Err(error) => result.diagnostics.push(Diagnostic {
                path,
                message: error.to_string(),
            }),
        }
    }
    result
        .findings
        .sort_by(|a, b| (&a.file, a.line, &a.rule).cmp(&(&b.file, b.line, &b.rule)));
    Ok(result)
}
