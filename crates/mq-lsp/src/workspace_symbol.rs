use std::path::{Path, PathBuf};
use std::str::FromStr;

use bimap::BiMap;
use rustc_hash::FxHashSet;
use tower_lsp_server::ls_types::{self, Location, OneOf, Position, Range, SymbolKind, SymbolTag, WorkspaceSymbol};
use url::Url;

/// Maximum number of symbols returned for an empty query.
const EMPTY_QUERY_LIMIT: usize = 1000;
/// Maximum number of unopened files indexed per request.
const MAX_DISK_FILES: usize = 2000;
const SKIPPED_DIRS: [&str; 3] = ["target", "node_modules", "dist"];

/// Symbols from `*.mq` files under `roots` that are not in `open`, capped for an empty query.
pub(crate) fn disk_symbols(
    roots: &[PathBuf],
    open: &FxHashSet<String>,
    query: &str,
    remaining: usize,
) -> Vec<WorkspaceSymbol> {
    let mut files = Vec::new();
    let mut seen = FxHashSet::default();
    for root in roots {
        collect_mq_files(root, &mut files, &mut seen);
    }

    let mut out = Vec::new();
    for path in files {
        if out.len() >= remaining {
            break;
        }
        let Ok(path) = std::path::absolute(&path) else { continue };
        let Ok(url) = Url::from_file_path(&path) else { continue };
        if open.contains(url.as_str()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut hir = mq_hir::Hir::default();
        let (source_id, _, _) = hir.add_code(Some(url.clone()), &text);
        let mut source_map = BiMap::new();
        source_map.insert(url.to_string(), source_id);
        out.extend(collect(&hir, query, &source_map));
    }
    out.truncate(remaining);
    out
}

fn collect_mq_files(dir: &Path, files: &mut Vec<PathBuf>, seen: &mut FxHashSet<PathBuf>) {
    if files.len() >= MAX_DISK_FILES {
        return;
    }
    if dir.is_file() {
        if dir.extension().is_some_and(|e| e == "mq") && seen.insert(dir.to_path_buf()) {
            files.push(dir.to_path_buf());
        }
        return;
    }
    if !seen.insert(dir.to_path_buf()) {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        // Symlinked directories are not followed, which rules out cycles.
        let is_real_dir = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir());
        if is_real_dir {
            if name.starts_with('.') || SKIPPED_DIRS.contains(&name) {
                continue;
            }
            collect_mq_files(&path, files, seen);
        } else if path.is_file() && path.extension().is_some_and(|e| e == "mq") && seen.insert(path.clone()) {
            files.push(path);
        }
        if files.len() >= MAX_DISK_FILES {
            return;
        }
    }
}

pub(crate) fn collect(
    hir_guard: &mq_hir::Hir,
    query: &str,
    source_map: &BiMap<String, mq_hir::SourceId>,
) -> Vec<WorkspaceSymbol> {
    let query = query.to_lowercase();

    hir_guard
        .symbols()
        .filter_map(|(_, symbol)| {
            let name = symbol.value.as_ref()?;
            if name.is_empty() || (!query.is_empty() && !name.to_lowercase().contains(&query)) {
                return None;
            }

            let kind = match &symbol.kind {
                mq_hir::SymbolKind::Function(_) => SymbolKind::FUNCTION,
                mq_hir::SymbolKind::Variable | mq_hir::SymbolKind::DestructuringBinding => SymbolKind::FIELD,
                mq_hir::SymbolKind::String => SymbolKind::STRING,
                mq_hir::SymbolKind::Boolean => SymbolKind::BOOLEAN,
                mq_hir::SymbolKind::None => SymbolKind::NULL,
                _ => return None,
            };

            let text_range = symbol.source.text_range?;
            let source_id = symbol.source.source_id?;
            let url = source_map.get_by_right(&source_id)?;
            let is_deprecated = symbol.is_deprecated();

            Some(WorkspaceSymbol {
                name: name.to_string(),
                kind,
                tags: if is_deprecated {
                    Some(vec![SymbolTag::DEPRECATED])
                } else {
                    None
                },
                container_name: None,
                location: OneOf::Left(Location {
                    uri: ls_types::Uri::from_str(url).unwrap(),
                    range: Range {
                        start: Position {
                            line: text_range.start.line - 1,
                            character: (text_range.start.column - 1),
                        },
                        end: Position {
                            line: text_range.end.line - 1,
                            character: (text_range.end.column - 1),
                        },
                    },
                }),
                data: None,
            })
        })
        .collect::<Vec<_>>()
}

/// Merges open-document and disk symbols, capping empty queries.
pub(crate) fn merge(mut open: Vec<WorkspaceSymbol>, disk: Vec<WorkspaceSymbol>, query: &str) -> Vec<WorkspaceSymbol> {
    open.extend(disk);
    if query.is_empty() {
        open.truncate(EMPTY_QUERY_LIMIT);
    }
    open
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_symbols_skips_open_and_caps() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.mq"), "def disk_a(): 1;").unwrap();
        std::fs::write(dir.path().join("b.mq"), "def disk_b(): 2;").unwrap();
        std::fs::create_dir(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target/c.mq"), "def disk_c(): 3;").unwrap();

        let roots = vec![dir.path().to_path_buf()];
        let mut open = FxHashSet::default();
        let all = disk_symbols(&roots, &open, "", 10);
        assert_eq!(all.len(), 2);

        open.insert(Url::from_file_path(dir.path().join("a.mq")).unwrap().to_string());
        let rest = disk_symbols(&roots, &open, "disk", 10);
        assert_eq!(rest.len(), 1);
        assert_eq!(rest[0].name, "disk_b");

        assert_eq!(disk_symbols(&roots, &FxHashSet::default(), "", 1).len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_disk_symbols_ignores_symlink_cycles() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.mq"), "def disk_main(): 1;").unwrap();
        let cache = dir.path().join("cache");
        std::fs::create_dir(&cache).unwrap();
        std::os::unix::fs::symlink(".", cache.join("a")).unwrap();
        std::os::unix::fs::symlink(".", cache.join("b")).unwrap();

        let mut files = Vec::new();
        collect_mq_files(dir.path(), &mut files, &mut FxHashSet::default());
        assert_eq!(files, vec![dir.path().join("main.mq")]);
    }

    #[test]
    fn test_disk_symbols_relative_root() {
        let dir = tempfile::tempdir_in(".").unwrap();
        std::fs::write(dir.path().join("a.mq"), "def rel_a(): 1;").unwrap();
        let rel = dir.path().file_name().map(PathBuf::from).unwrap();
        let symbols = disk_symbols(&[rel], &FxHashSet::default(), "rel_a", 10);
        assert_eq!(symbols.len(), 1);
    }

    #[test]
    fn test_response_with_empty_hir() {
        let hir = mq_hir::Hir::default();
        let source_map = BiMap::new();
        assert!(collect(&hir, "", &source_map).is_empty());
    }

    #[test]
    fn test_response_matches_across_multiple_sources() {
        let mut hir = mq_hir::Hir::default();
        let url1 = Url::parse("file:///a.mq").unwrap();
        let url2 = Url::parse("file:///b.mq").unwrap();

        let (source_id1, _, _) = hir.add_code(Some(url1.clone()), "def func_a(): 1;");
        let (source_id2, _, _) = hir.add_code(Some(url2.clone()), "def func_b(): 2; | let var_b = 3");

        let mut source_map = BiMap::new();
        source_map.insert(url1.to_string(), source_id1);
        source_map.insert(url2.to_string(), source_id2);

        let symbols = collect(&hir, "func", &source_map);
        assert_eq!(symbols.len(), 2);
        assert!(symbols.iter().all(|s| s.kind == SymbolKind::FUNCTION));

        let symbols = collect(&hir, "var_b", &source_map);
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "var_b");

        assert!(collect(&hir, "does_not_exist", &source_map).is_empty());
    }
}
