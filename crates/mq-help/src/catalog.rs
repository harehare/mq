use colored::Colorize;
use mq_lang::{BUILTIN_FUNCTION_DOC, BUILTIN_MODULE_FILE, BUILTIN_SELECTOR_DOC, STANDARD_MODULES};
use serde::Serialize;

use crate::reference;

/// A single documented parameter, as shown by `mq help`.
#[derive(Debug, Clone, Serialize)]
pub struct HelpParam {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
}

/// A runnable example paired with its verified expected output.
#[derive(Debug, Clone, Serialize)]
pub struct HelpExample {
    pub code: String,
    pub expected: String,
}

/// Full documentation for one function or selector, unifying native builtins, selectors,
/// `builtin.mq` functions, and standard-module functions behind a single shape — the same
/// shape can be served by `mq help --json` and by `mq-web-api`'s `/functions/{name}` and
/// `/selectors/{name}` endpoints, so CLI and Web API can't drift apart.
#[derive(Debug, Clone, Serialize)]
pub struct HelpEntry {
    pub name: String,
    pub kind: &'static str,
    pub params: Vec<HelpParam>,
    pub returns: String,
    pub description: String,
    pub examples: Vec<HelpExample>,
    pub capability: Option<String>,
    pub related_module: Option<String>,
}

/// Looks up every entry matching `name` — usually one, but a name may be defined in more
/// than one standard module, in which case every match is returned. `name` may be qualified
/// as `module::function` to disambiguate a function whose name collides with its own module
/// (e.g. `section::section`, distinct from the `section` module itself).
pub fn lookup(name: &str) -> Vec<HelpEntry> {
    if let Some((module, function)) = name.split_once("::") {
        return all_entries()
            .into_iter()
            .filter(|e| e.name == function && e.related_module.as_deref() == Some(module))
            .collect();
    }

    let selector_name = if name.starts_with('.') {
        name.to_string()
    } else {
        format!(".{name}")
    };

    all_entries()
        .into_iter()
        .filter(|e| e.name == name || (e.kind == "selector" && e.name == selector_name))
        .collect()
}

/// Every documented function and selector: native builtins, selectors, `builtin.mq`
/// functions, and every standard module's functions.
pub fn all_entries() -> Vec<HelpEntry> {
    let mut results = top_level_entries();
    for module in all_modules() {
        results.extend(module.functions);
    }
    results
}

/// Native builtin functions, selectors, and `builtin.mq` functions — everything in
/// [`all_entries`] except standard-module functions. Cheap: unlike `all_entries`/`all_modules`,
/// it never parses a standard module's source.
pub fn top_level_entries() -> Vec<HelpEntry> {
    let mut results = Vec::new();

    let mut fn_names: Vec<_> = BUILTIN_FUNCTION_DOC.keys().collect();
    fn_names.sort();
    for name in fn_names {
        let doc = &BUILTIN_FUNCTION_DOC[name];
        results.push(HelpEntry {
            name: name.to_string(),
            kind: "function",
            params: zip_params(doc.params, doc.param_types),
            returns: doc.returns.to_string(),
            description: doc.description.to_string(),
            examples: doc
                .examples
                .iter()
                .map(|e| HelpExample {
                    code: e.code.to_string(),
                    expected: e.expected.to_string(),
                })
                .collect(),
            capability: doc.capability.map(str::to_string),
            related_module: None,
        });
    }

    let mut selector_names: Vec<_> = BUILTIN_SELECTOR_DOC.keys().collect();
    selector_names.sort();
    for name in selector_names {
        let doc = &BUILTIN_SELECTOR_DOC[name];
        results.push(HelpEntry {
            name: name.to_string(),
            kind: "selector",
            params: zip_params(doc.params, doc.param_types),
            returns: doc.returns.to_string(),
            description: doc.description.to_string(),
            examples: doc
                .examples
                .iter()
                .map(|e| HelpExample {
                    code: e.code.to_string(),
                    expected: e.expected.to_string(),
                })
                .collect(),
            capability: doc.capability.map(str::to_string),
            related_module: None,
        });
    }

    for fdoc in reference::extract_functions_from_cst(BUILTIN_MODULE_FILE, true) {
        results.push(from_mq_fn_doc(fdoc, None));
    }

    results
}

/// Every function/selector name known to `mq help`, used both to list everything (`mq help`
/// with no name) and as candidates for "did you mean" suggestions.
pub fn all_names() -> Vec<String> {
    let mut names: Vec<String> = all_entries().into_iter().map(|e| e.name).collect();
    names.sort();
    names.dedup();
    names
}

/// A standard module's header doc (if any) plus the functions it defines.
#[derive(Debug, Clone, Serialize)]
pub struct HelpModule {
    pub name: String,
    pub description: String,
    pub examples: Vec<HelpExample>,
    pub functions: Vec<HelpEntry>,
}

/// Every standard module, with its header doc (if any) and function list.
pub fn all_modules() -> Vec<HelpModule> {
    let mut modules: Vec<_> = STANDARD_MODULES.iter().collect();
    modules.sort_by_key(|(k, _)| *k);

    modules
        .into_iter()
        .map(|(module_name, get_source)| {
            let (doc, fdocs) = reference::extract_module(get_source(), false);
            let functions = fdocs
                .into_iter()
                .map(|fdoc| from_mq_fn_doc(fdoc, Some(module_name.to_string())))
                .collect();

            HelpModule {
                name: module_name.to_string(),
                description: doc.as_ref().map(|d| d.description.clone()).unwrap_or_default(),
                examples: doc
                    .map(|d| {
                        d.examples
                            .into_iter()
                            .map(|e| HelpExample {
                                code: e.code,
                                expected: e.expected,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                functions,
            }
        })
        .collect()
}

/// Looks up a single standard module by name (e.g. `section`, `table`).
pub fn lookup_module(name: &str) -> Option<HelpModule> {
    all_modules().into_iter().find(|m| m.name == name)
}

/// A "did you mean" suggestion for a name that wasn't found, or `None` if nothing is close.
pub fn suggest(name: &str) -> Option<String> {
    if name.starts_with('.') {
        return mq_lang::suggest_selector(name).map(str::to_string);
    }
    let mut candidates = all_names();
    candidates.extend(STANDARD_MODULES.keys().map(|k| k.to_string()));
    mq_lang::suggest_name(name, candidates.iter().map(String::as_str))
}

fn from_mq_fn_doc(fdoc: reference::MqFnDoc, related_module: Option<String>) -> HelpEntry {
    HelpEntry {
        name: fdoc.name,
        kind: "function",
        params: fdoc
            .params
            .into_iter()
            .map(|name| HelpParam {
                name,
                type_name: "dynamic".to_string(),
            })
            .collect(),
        returns: fdoc.returns.unwrap_or_else(|| "dynamic".to_string()),
        description: fdoc.description,
        examples: fdoc
            .examples
            .into_iter()
            .map(|e| HelpExample {
                code: e.code,
                expected: e.expected,
            })
            .collect(),
        capability: None,
        related_module,
    }
}

fn zip_params(names: &[&'static str], types: &[&'static str]) -> Vec<HelpParam> {
    names
        .iter()
        .enumerate()
        .map(|(i, name)| HelpParam {
            name: name.to_string(),
            type_name: types.get(i).copied().unwrap_or("dynamic").to_string(),
        })
        .collect()
}

/// Renders a single entry as colored, human-readable text.
pub fn render_human(entry: &HelpEntry) -> String {
    let mut out = String::new();

    let params = entry
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.type_name))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!(
        "{} {}\n  {}({}): {}\n",
        entry.name.bold().cyan(),
        format!("({})", entry.kind).dimmed(),
        entry.name,
        params,
        entry.returns
    ));

    if !entry.description.is_empty() {
        out.push_str(&format!("\n  {}\n", entry.description));
    }

    if let Some(module) = &entry.related_module {
        out.push_str(&format!(
            "\n  {} import \"{}\" | {}::{}(...)\n",
            "Module:".bold(),
            module,
            module,
            entry.name
        ));
    }

    if let Some(capability) = &entry.capability {
        out.push_str(&format!(
            "\n  {} requires the `{}` capability (not available via the hosted Web API/playground)\n",
            "Capability:".bold().yellow(),
            capability
        ));
    }

    if !entry.examples.is_empty() {
        out.push_str(&format!("\n  {}\n", "Examples:".bold()));
        for example in &entry.examples {
            out.push_str(&format!(
                "    {}\n    {} {}\n",
                example.code.green(),
                "#=>".dimmed(),
                example.expected
            ));
        }
    }

    out
}

/// Renders a module overview as colored, human-readable text: its header doc, examples, and
/// the list of functions it defines.
pub fn render_module_human(module: &HelpModule) -> String {
    let mut out = String::new();

    out.push_str(&format!("{} {}\n", module.name.bold().cyan(), "(module)".dimmed()));

    if !module.description.is_empty() {
        out.push_str(&format!("\n  {}\n", module.description));
    }

    out.push_str(&format!(
        "\n  {} import \"{}\" | {}::<function>(...)\n",
        "Usage:".bold(),
        module.name,
        module.name
    ));

    if !module.examples.is_empty() {
        out.push_str(&format!("\n  {}\n", "Examples:".bold()));
        for example in &module.examples {
            out.push_str(&format!(
                "    {}\n    {} {}\n",
                example.code.green(),
                "#=>".dimmed(),
                example.expected
            ));
        }
    }

    if !module.functions.is_empty() {
        out.push_str(&format!("\n  {}\n", "Functions:".bold()));
        for f in &module.functions {
            let params = f
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name, p.type_name))
                .collect::<Vec<_>>()
                .join(", ");
            let summary = f.description.split(". ").next().unwrap_or_default();
            out.push_str(&format!(
                "    {}({}): {}{}\n",
                f.name.green(),
                params,
                f.returns,
                if summary.is_empty() {
                    String::new()
                } else {
                    format!(" — {summary}")
                }
            ));
        }
        out.push_str(&format!(
            "\n  Run `mq help {}::<function>` for a function's params, examples, and full description.\n",
            module.name
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_native_function() {
        let entries = lookup("map");
        assert!(entries.iter().any(|e| e.kind == "function" && e.name == "map"));
    }

    #[test]
    fn test_lookup_selector_without_dot() {
        let entries = lookup("h1");
        assert!(entries.iter().any(|e| e.kind == "selector" && e.name == ".h1"));
    }

    #[test]
    fn test_lookup_selector_with_dot() {
        let entries = lookup(".h1");
        assert!(entries.iter().any(|e| e.kind == "selector" && e.name == ".h1"));
    }

    #[test]
    fn test_lookup_qualified_disambiguates_module_and_function() {
        // `section` names both the module and a function defined inside it.
        let qualified = lookup("section::section");
        assert!(
            qualified
                .iter()
                .any(|e| e.name == "section" && e.related_module.as_deref() == Some("section"))
        );
    }

    #[test]
    fn test_lookup_module_finds_section_and_table() {
        let section = lookup_module("section").expect("section module should be documented");
        assert!(!section.description.is_empty());
        assert!(section.functions.iter().any(|f| f.name == "section"));

        let table = lookup_module("table").expect("table module should be documented");
        assert!(!table.description.is_empty());
        assert!(table.functions.iter().any(|f| f.name == "tables"));
    }

    #[test]
    fn test_lookup_module_unknown_returns_none() {
        assert!(lookup_module("definitely_not_a_real_module").is_none());
    }

    #[test]
    fn test_suggest_typo_includes_module_names() {
        assert_eq!(suggest("sction"), Some("section".to_string()));
    }

    #[test]
    fn test_lookup_unknown_returns_empty() {
        assert!(lookup("definitely_not_a_real_function_name").is_empty());
    }

    #[test]
    fn test_lookup_module_function_has_related_module() {
        let entries = lookup("csv_parse");
        assert!(entries.iter().any(|e| e.related_module.as_deref() == Some("csv")));
    }

    #[test]
    fn test_all_names_contains_known_symbols() {
        let names = all_names();
        assert!(names.contains(&"map".to_string()));
        assert!(names.contains(&".h1".to_string()));
    }

    #[test]
    fn test_suggest_typo() {
        assert_eq!(suggest("mpa"), Some("map".to_string()));
    }

    /// Every example shown by `mq help` is a real, runnable snippet — this evaluates each
    /// one through the actual engine and checks it against its recorded `expected` output,
    /// so a stale/wrong example fails CI instead of silently misleading a reader.
    #[test]
    fn test_doc_examples_are_correct() {
        let mut failures = Vec::new();

        for entry in all_entries() {
            for example in &entry.examples {
                let mut engine = mq_lang::DefaultEngine::default();
                engine.load_builtin_module();
                let input = vec![mq_lang::RuntimeValue::NONE].into_iter();
                match engine.eval(&example.code, input) {
                    Ok(values) => {
                        let rendered = values.into_iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
                        if rendered != example.expected {
                            failures.push(format!(
                                "{} `{}`: expected `{}`, got `{}`",
                                entry.name, example.code, example.expected, rendered
                            ));
                        }
                    }
                    Err(e) => {
                        failures.push(format!("{} `{}`: eval error: {}", entry.name, example.code, e));
                    }
                }
            }
        }

        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }

    /// Same guarantee as `test_doc_examples_are_correct`, for module-level header examples.
    #[test]
    fn test_module_doc_examples_are_correct() {
        let mut failures = Vec::new();

        for module in all_modules() {
            for example in &module.examples {
                let mut engine = mq_lang::DefaultEngine::default();
                engine.load_builtin_module();
                let input = vec![mq_lang::RuntimeValue::NONE].into_iter();
                match engine.eval(&example.code, input) {
                    Ok(values) => {
                        let rendered = values.into_iter().map(|v| v.to_string()).collect::<Vec<_>>().join(", ");
                        if rendered != example.expected {
                            failures.push(format!(
                                "{} `{}`: expected `{}`, got `{}`",
                                module.name, example.code, example.expected, rendered
                            ));
                        }
                    }
                    Err(e) => {
                        failures.push(format!("{} `{}`: eval error: {}", module.name, example.code, e));
                    }
                }
            }
        }

        assert!(failures.is_empty(), "\n{}", failures.join("\n"));
    }
}
