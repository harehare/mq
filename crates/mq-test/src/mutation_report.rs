//! Renders mutation testing results as text, JSON, Markdown or HTML.

use std::path::PathBuf;

use rustc_hash::FxHashMap;

use crate::coverage::{HTML_STYLE, badge_class};
use crate::highlight;
use crate::html;
use crate::mutation::Mutant;

/// Report format for `--mutate`.
#[derive(clap::ValueEnum, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MutationFormat {
    /// Survivors and the score, for the terminal.
    #[default]
    Text,
    /// Machine-readable JSON report.
    Json,
    /// Summary table and the survivors as diffs.
    Markdown,
    /// Self-contained HTML report with highlighted source.
    Html,
}

/// A mutant and whether the tests caught it.
pub(crate) struct MutantResult {
    pub(crate) mutant: Mutant,
    pub(crate) killed: bool,
}

/// Results for the mutants of one module, in source order.
pub(crate) struct ModuleResult {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) mutants: Vec<MutantResult>,
}

impl ModuleResult {
    fn killed(&self) -> usize {
        self.mutants.iter().filter(|m| m.killed).count()
    }

    fn survivors(&self) -> impl Iterator<Item = &Mutant> {
        self.mutants.iter().filter(|m| !m.killed).map(|m| &m.mutant)
    }

    /// The source line of `mutant` before and after the replacement.
    fn changed_line(&self, mutant: &Mutant) -> (String, String) {
        let line = |source: &str| source.lines().nth(mutant.line - 1).unwrap_or_default().to_string();
        (line(&self.source), line(&mutant.apply(&self.source)))
    }
}

fn percent(killed: usize, total: usize) -> f64 {
    if total == 0 {
        100.0
    } else {
        killed as f64 * 100.0 / total as f64
    }
}

/// Percent of mutants killed across `modules`; 100 when there are none.
pub(crate) fn score(modules: &[ModuleResult]) -> f64 {
    percent(
        modules.iter().map(ModuleResult::killed).sum(),
        modules.iter().map(|m| m.mutants.len()).sum(),
    )
}

fn totals(modules: &[ModuleResult]) -> (usize, usize) {
    let total = modules.iter().map(|m| m.mutants.len()).sum();
    (modules.iter().map(ModuleResult::killed).sum(), total)
}

/// Renders `modules` in `format`.
pub(crate) fn format_report(modules: &[ModuleResult], format: MutationFormat) -> String {
    match format {
        MutationFormat::Text => format_text(modules),
        MutationFormat::Json => format_json(modules),
        MutationFormat::Markdown => format_markdown(modules),
        MutationFormat::Html => format_html(modules),
    }
}

fn format_text(modules: &[ModuleResult]) -> String {
    let (killed, total) = totals(modules);
    let mut out = String::from("# Mutation testing\n\n");
    if total > 0 && killed < total {
        out.push_str("Survived mutants:\n");
        for module in modules {
            for m in module.survivors() {
                out.push_str(&format!(
                    "  {}:{}:{}  {}\n",
                    module.path.display(),
                    m.line,
                    m.column,
                    m.describe()
                ));
            }
        }
        out.push('\n');
    }
    if total == 0 {
        out.push_str("No mutants were generated.\n");
    } else {
        out.push_str(&format!(
            "Mutation score: {:.1}% ({killed} killed, {} survived, {total} total)\n",
            score(modules),
            total - killed
        ));
    }
    out
}

fn format_json(modules: &[ModuleResult]) -> String {
    let (killed, total) = totals(modules);
    let mutants: Vec<serde_json::Value> = modules
        .iter()
        .flat_map(|module| {
            module.mutants.iter().map(|r| {
                serde_json::json!({
                    "file": module.path.display().to_string(),
                    "line": r.mutant.line,
                    "column": r.mutant.column,
                    "original": r.mutant.original,
                    "replacement": r.mutant.replacement,
                    "killed": r.killed,
                })
            })
        })
        .collect();
    serde_json::json!({
        "score": score(modules),
        "killed": killed,
        "survived": total - killed,
        "mutants": mutants,
    })
    .to_string()
}

fn format_markdown(modules: &[ModuleResult]) -> String {
    let mut out =
        String::from("# Mutation report\n\n| File | Score | Killed | Survived |\n| --- | --- | --- | --- |\n");
    for module in modules {
        let killed = module.killed();
        out.push_str(&format!(
            "| `{}` | {:.1}% | {killed} | {} |\n",
            module.path.display(),
            percent(killed, module.mutants.len()),
            module.mutants.len() - killed
        ));
    }
    let (killed, total) = totals(modules);
    out.push_str(&format!(
        "\n**Total: {:.1}% ({killed}/{total} killed)**\n",
        score(modules)
    ));

    for module in modules.iter().filter(|m| m.survivors().next().is_some()) {
        out.push_str(&format!("\n## {}\n", module.path.display()));
        for mutant in module.survivors() {
            let (before, after) = module.changed_line(mutant);
            out.push_str(&format!(
                "\nLine {}:{}, {}\n\n```diff\n-{before}\n+{after}\n```\n",
                mutant.line,
                mutant.column,
                mutant.describe()
            ));
        }
    }
    out
}

const EXTRA_STYLE: &str = "
table.source td.note { color: var(--badge-low-fg); font-size: 0.8rem; white-space: normal; }
";

fn format_html(modules: &[ModuleResult]) -> String {
    let (killed, total) = totals(modules);
    let mut rows = String::new();
    let mut sources = String::new();

    for (i, module) in modules.iter().enumerate() {
        let module_killed = module.killed();
        let pct = percent(module_killed, module.mutants.len());
        let anchor = format!("file-{i}");
        let file = html::escape(&module.path.display().to_string());
        rows.push_str(&format!(
            "      <tr><td><a href=\"#{anchor}\">{file}</a></td><td><span class=\"badge {}\">{pct:.1}%</span></td><td>{module_killed}/{}</td></tr>\n",
            badge_class(pct),
            module.mutants.len()
        ));

        let mut by_line: FxHashMap<usize, Vec<&MutantResult>> = FxHashMap::default();
        for result in &module.mutants {
            by_line.entry(result.mutant.line).or_default().push(result);
        }
        let mut lines_html = String::new();
        for (index, code) in highlight::highlight_lines(&module.source).into_iter().enumerate() {
            let line_no = index + 1;
            let results = by_line.get(&line_no);
            let class = match results {
                None => "plain",
                Some(r) if r.iter().all(|r| r.killed) => "covered",
                Some(_) => "uncovered",
            };
            let note = results
                .into_iter()
                .flatten()
                .filter(|r| !r.killed)
                .map(|r| html::escape(&r.mutant.describe()))
                .collect::<Vec<_>>()
                .join("<br>");
            lines_html.push_str(&format!(
                "          <tr class=\"{class}\"><td class=\"lineno\">{line_no}</td><td class=\"code\">{code}</td><td class=\"note\">{note}</td></tr>\n"
            ));
        }
        sources.push_str(&format!(
            "      <details id=\"{anchor}\">\n        <summary>{file} - {pct:.1}% ({module_killed}/{})</summary>\n        <table class=\"source\">\n          <tbody>\n{lines_html}          </tbody>\n        </table>\n      </details>\n",
            module.mutants.len()
        ));
    }

    let overall = score(modules);
    let body = format!(
        "  <h1>Mutation report</h1>\n  <table>\n    <thead>\n      <tr><th>File</th><th>Score</th><th>Killed</th></tr>\n    </thead>\n    <tbody>\n{rows}    </tbody>\n    <tfoot>\n      <tr><td>Total</td><td><span class=\"badge {}\">{overall:.1}%</span></td><td>{killed}/{total}</td></tr>\n    </tfoot>\n  </table>\n{sources}",
        badge_class(overall)
    );
    html::page("mq-test mutation report", &format!("{HTML_STYLE}{EXTRA_STYLE}"), &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutation::generate;

    fn sample() -> Vec<ModuleResult> {
        let source = "def f(x): x < 1;\ndef g(x): x == 2;\n".to_string();
        let mutants = generate(&source, |_| true)
            .into_iter()
            .map(|mutant| MutantResult {
                killed: mutant.line == 1,
                mutant,
            })
            .collect();
        vec![ModuleResult {
            path: PathBuf::from("lib.mq"),
            source,
            mutants,
        }]
    }

    #[test]
    fn test_score_and_text_report() {
        let modules = sample();
        assert_eq!(score(&modules), 50.0);
        let text = format_report(&modules, MutationFormat::Text);
        assert!(text.contains("lib.mq:2:13  `==` -> `!=`"));
        assert!(text.contains("Mutation score: 50.0% (2 killed, 2 survived, 4 total)"));
    }

    #[test]
    fn test_markdown_report_shows_survivors_as_diffs() {
        let md = format_report(&sample(), MutationFormat::Markdown);
        assert!(md.contains("| `lib.mq` | 50.0% | 2 | 2 |"));
        assert!(md.contains("```diff\n-def g(x): x == 2;\n+def g(x): x != 2;\n```"));
    }

    #[test]
    fn test_html_report_marks_survivor_lines() {
        let html = format_report(&sample(), MutationFormat::Html);
        assert!(html.contains("<tr class=\"covered\"><td class=\"lineno\">1</td>"));
        assert!(html.contains("<tr class=\"uncovered\"><td class=\"lineno\">2</td>"));
        assert!(html.contains("`==` -&gt; `!=`"));
    }

    #[test]
    fn test_json_report_without_mutants() {
        let json: serde_json::Value = serde_json::from_str(&format_report(&[], MutationFormat::Json)).unwrap();
        assert_eq!(json["score"], 100.0);
        assert_eq!(json["mutants"].as_array().unwrap().len(), 0);
    }
}
