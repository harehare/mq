//! `mq module search`: query the published module index.
use super::Cli;
use colored::Colorize;
use miette::{IntoDiagnostic, WrapErr, bail, miette};
use mq_lang::UreqFetcher;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const INDEX_URL: &str = "https://mqlang.org/modules.json";
const INDEX_VERSION: u64 = 1;
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    name: String,
    repo: String,
    category: String,
    /// Inline Markdown, e.g. `[JSON5](https://json5.org): comments`.
    description: String,
}

#[derive(Debug)]
struct Index {
    categories: Vec<(String, String)>,
    modules: Vec<Entry>,
}

impl Index {
    fn category_label<'a>(&'a self, slug: &'a str) -> &'a str {
        self.categories
            .iter()
            .find(|(s, _)| s == slug)
            .map_or(slug, |(_, label)| label.as_str())
    }
}

fn parse_index(text: &str) -> miette::Result<Index> {
    let value: Value = serde_json::from_str(text)
        .into_diagnostic()
        .wrap_err("Invalid module index")?;
    match value["version"].as_u64() {
        Some(INDEX_VERSION) => {}
        other => return Err(miette!("Unsupported module index version {other:?}, upgrade mq")),
    }
    let categories = value["categories"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|c| Some((c["slug"].as_str()?.to_string(), c["label"].as_str()?.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let modules = value["modules"]
        .as_array()
        .ok_or_else(|| miette!("Module index has no `modules`"))?
        .iter()
        .map(|module| {
            let field = |key: &str| {
                module[key]
                    .as_str()
                    .map(str::to_string)
                    .ok_or_else(|| miette!("Module index entry is missing `{key}`"))
            };
            Ok(Entry {
                name: field("name")?,
                repo: field("repo")?,
                category: field("category")?,
                description: field("description")?,
            })
        })
        .collect::<miette::Result<Vec<_>>>()?;
    Ok(Index { categories, modules })
}

/// Drops the `[text](url)` link syntax, keeping `text`.
fn plain_text(markdown: &str) -> String {
    let mut out = String::new();
    let mut rest = markdown;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let link = tail
            .find("](")
            .and_then(|mid| tail[mid + 2..].find(')').map(|end| (mid, mid + 2 + end)));
        match link {
            Some((mid, end)) => {
                out.push_str(&tail[..mid]);
                rest = &tail[end + 1..];
            }
            None => {
                out.push('[');
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Entries matching every term (case-insensitive, over name, category and description).
fn search<'a>(index: &'a Index, terms: &[String], category: Option<&str>) -> Vec<&'a Entry> {
    let terms: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
    index
        .modules
        .iter()
        .filter(|entry| category.is_none_or(|c| entry.category == c))
        .filter(|entry| {
            let haystack = format!(
                "{} {} {}",
                entry.name,
                index.category_label(&entry.category),
                plain_text(&entry.description)
            )
            .to_lowercase();
            terms.iter().all(|term| haystack.contains(term))
        })
        .collect()
}

fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("mq").join("modules.json"))
}

fn is_fresh(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age < CACHE_TTL)
}

fn read_cached(path: &Path) -> Option<Index> {
    parse_index(&fs::read_to_string(path).ok()?).ok()
}

/// The index from a fresh cache, else the network, else a stale cache.
fn load_index(refresh: bool) -> miette::Result<Index> {
    let cache = cache_path();
    if !refresh
        && let Some(path) = cache.as_deref().filter(|p| is_fresh(p))
        && let Some(index) = read_cached(path)
    {
        return Ok(index);
    }

    match UreqFetcher::default().fetch_uncached(INDEX_URL) {
        Ok(text) => {
            let index = parse_index(&text)?;
            if let Some(path) = &cache {
                // The cache is only an optimization, so a failed write is not an error.
                let _ = path.parent().map(fs::create_dir_all);
                let _ = fs::write(path, text);
            }
            Ok(index)
        }
        Err(e) => match cache.as_deref().and_then(read_cached) {
            Some(index) => {
                eprintln!("warning: could not fetch {INDEX_URL}, using the cached index ({e})");
                Ok(index)
            }
            None => Err(miette!("Could not fetch {INDEX_URL}: {e}")),
        },
    }
}

impl Cli {
    /// `mq module search [QUERY...] [--category SLUG] [--json] [--refresh]`
    pub(super) fn module_search(
        terms: &[String],
        category: Option<&str>,
        json: bool,
        refresh: bool,
    ) -> miette::Result<()> {
        let index = load_index(refresh)?;
        if let Some(slug) = category
            && !index.categories.iter().any(|(s, _)| s == slug)
        {
            let valid: Vec<&str> = index.categories.iter().map(|(s, _)| s.as_str()).collect();
            bail!("Unknown category `{slug}`, expected one of: {}", valid.join(", "));
        }
        let found = search(&index, terms, category);

        if json {
            let entries: Vec<Value> = found
                .iter()
                .map(|e| {
                    json!({
                        "name": e.name,
                        "repo": e.repo,
                        "category": e.category,
                        "description": e.description,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&entries).into_diagnostic()?);
            return Ok(());
        }

        if found.is_empty() {
            eprintln!("No modules found");
            return Ok(());
        }
        let repo_width = found.iter().map(|e| e.repo.len()).max().unwrap_or(0);
        let label_width = found
            .iter()
            .map(|e| index.category_label(&e.category).chars().count())
            .max()
            .unwrap_or(0);
        for entry in found {
            let label = index.category_label(&entry.category);
            println!(
                "{}  {}  {}",
                format!("{:<repo_width$}", entry.repo).bold().cyan(),
                format!("{label:<label_width$}").dimmed(),
                plain_text(&entry.description)
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    const INDEX: &str = include_str!("../../../../docs/modules.json");

    fn index() -> Index {
        parse_index(INDEX).unwrap()
    }

    #[rstest]
    #[case("[JSON5](https://json5.org): comments", "JSON5: comments")]
    #[case("[a](u) and [b](v)", "a and b")]
    #[case("no links, [brackets] only", "no links, [brackets] only")]
    #[case("broken [link](oops", "broken [link](oops")]
    fn test_plain_text(#[case] markdown: &str, #[case] expected: &str) {
        assert_eq!(plain_text(markdown), expected);
    }

    #[test]
    fn test_parse_index_rejects_unknown_version() {
        assert!(parse_index(r#"{"version": 2, "modules": []}"#).is_err());
    }

    #[test]
    fn test_parse_index_rejects_missing_field() {
        assert!(parse_index(r#"{"version": 1, "modules": [{"name": "a"}]}"#).is_err());
    }

    #[test]
    fn test_search_matches_all_terms_case_insensitively() {
        let index = index();
        let names = |terms: &[&str]| -> Vec<String> {
            let terms: Vec<String> = terms.iter().map(|t| t.to_string()).collect();
            search(&index, &terms, None).iter().map(|e| e.name.clone()).collect()
        };

        assert!(names(&["KDL"]).contains(&"kdl".to_string()));
        assert!(names(&["kdl", "no-such-word"]).is_empty());
        assert_eq!(names(&[]).len(), index.modules.len());
    }

    #[test]
    fn test_search_filters_by_category() {
        let index = index();
        let found = search(&index, &[], Some("format-parsers"));

        assert!(!found.is_empty());
        assert!(found.iter().all(|e| e.category == "format-parsers"));
    }

    #[test]
    fn test_published_index_is_well_formed() {
        let index = index();
        let mut names: Vec<&str> = index.modules.iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), index.modules.len(), "module names must be unique");
        for entry in &index.modules {
            assert!(
                index.categories.iter().any(|(slug, _)| *slug == entry.category),
                "{} has unknown category {}",
                entry.name,
                entry.category
            );
            assert!(
                entry.repo.starts_with("github.com/"),
                "{} has repo {}",
                entry.name,
                entry.repo
            );
        }
    }

    /// Until the docs table is generated from the index, the two must list the same modules.
    #[test]
    fn test_index_matches_docs_table() {
        let table = include_str!("../../../../docs/books/src/start/modules.md");
        let index = index();

        assert_eq!(table.matches("<tr data-category=").count(), index.modules.len());
        for entry in &index.modules {
            assert!(
                table.contains(&format!("https://{}\"", entry.repo)),
                "{} is missing from docs/books/src/start/modules.md",
                entry.repo
            );
        }
    }
}
