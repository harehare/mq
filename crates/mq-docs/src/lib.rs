use std::collections::VecDeque;

use itertools::Itertools;
use url::Url;

/// Documentation output format.
#[derive(Clone, Debug, Default, clap::ValueEnum)]
pub enum DocFormat {
    #[default]
    Markdown,
    Text,
    Html,
}

/// A group of documented symbols belonging to a single module or file.
struct ModuleDoc {
    name: String,
    symbols: VecDeque<[String; 4]>,
    selectors: VecDeque<[String; 2]>,
}

/// Generate documentation for mq functions, macros, and selectors.
///
/// If `module_names` or `files` is provided, only the specified modules/files are loaded.
/// Both can be combined. If `include_builtin` is true, built-in functions are also included.
/// Otherwise, all builtin functions are documented.
pub fn generate_docs(
    module_names: &Option<Vec<String>>,
    files: &Option<Vec<(String, String)>>,
    format: &DocFormat,
    include_builtin: bool,
) -> Result<String, miette::Error> {
    let has_files = files.as_ref().is_some_and(|f| !f.is_empty());
    let has_modules = module_names.as_ref().is_some_and(|m| !m.is_empty());

    let module_docs = if has_files || has_modules {
        let mut docs = Vec::new();

        if include_builtin {
            let mut hir = mq_hir::Hir::default();
            hir.add_code(None, "");
            docs.push(ModuleDoc {
                name: "Built-in".to_string(),
                symbols: extract_symbols(&hir),
                selectors: extract_selectors(&hir),
            });
        }

        if let Some(file_contents) = files {
            for (filename, content) in file_contents {
                let mut hir = mq_hir::Hir::default();
                hir.builtin.disabled = true;
                let url = Url::parse(&format!("file:///{filename}")).ok();
                hir.add_code(url, content);
                docs.push(ModuleDoc {
                    name: filename.clone(),
                    symbols: extract_symbols(&hir),
                    selectors: extract_selectors(&hir),
                });
            }
        }

        if let Some(module_names) = module_names {
            for module_name in module_names {
                let mut hir = mq_hir::Hir::default();
                hir.builtin.disabled = true;
                hir.add_code(None, &format!("include \"{module_name}\""));
                docs.push(ModuleDoc {
                    name: module_name.clone(),
                    symbols: extract_symbols(&hir),
                    selectors: extract_selectors(&hir),
                });
            }
        }

        docs
    } else {
        let mut hir = mq_hir::Hir::default();
        hir.add_code(None, "");
        vec![ModuleDoc {
            name: "Built-in functions and macros".to_string(),
            symbols: extract_symbols(&hir),
            selectors: extract_selectors(&hir),
        }]
    };

    match format {
        DocFormat::Markdown => format_markdown(&module_docs),
        DocFormat::Text => Ok(format_text(&module_docs)),
        DocFormat::Html => Ok(format_html(&module_docs)),
    }
}

/// Extract function and macro symbols from HIR.
fn extract_symbols(hir: &mq_hir::Hir) -> VecDeque<[String; 4]> {
    hir.symbols()
        .sorted_by_key(|(_, symbol)| symbol.value.clone())
        .filter_map(|(_, symbol)| match symbol {
            mq_hir::Symbol {
                kind: mq_hir::SymbolKind::Function(params),
                value: Some(value),
                doc,
                ..
            }
            | mq_hir::Symbol {
                kind: mq_hir::SymbolKind::Macro(params),
                value: Some(value),
                doc,
                ..
            } if !symbol.is_internal_function() => {
                let name = if symbol.is_deprecated() {
                    format!("~~`{}`~~", value)
                } else {
                    format!("`{}`", value)
                };
                let description = doc.iter().map(|(_, d)| d.to_string()).join("\n");
                let args = params.iter().map(|p| format!("`{}`", p.name)).join(", ");
                let example = format!("{}({})", value, params.iter().map(|p| p.name.as_str()).join(", "));

                Some([name, description, args, example])
            }
            _ => None,
        })
        .collect()
}

/// Extract selector symbols from HIR.
fn extract_selectors(hir: &mq_hir::Hir) -> VecDeque<[String; 2]> {
    hir.symbols()
        .sorted_by_key(|(_, symbol)| symbol.value.clone())
        .filter_map(|(_, symbol)| match symbol {
            mq_hir::Symbol {
                kind: mq_hir::SymbolKind::Selector,
                value: Some(value),
                doc,
                ..
            } => {
                let name = format!("`{}`", value);
                let description = doc.iter().map(|(_, d)| d.to_string()).join("\n");
                Some([name, description])
            }
            _ => None,
        })
        .collect()
}

/// Format documentation as a Markdown table.
fn format_markdown(module_docs: &[ModuleDoc]) -> Result<String, miette::Error> {
    let all_symbols: VecDeque<_> = module_docs.iter().flat_map(|m| m.symbols.iter()).cloned().collect();
    let all_selectors: VecDeque<_> = module_docs.iter().flat_map(|m| m.selectors.iter()).cloned().collect();

    let mut doc_csv = all_symbols
        .iter()
        .map(|[name, description, args, example]| {
            mq_lang::RuntimeValue::String([name, description, args, example].into_iter().join("\t"))
        })
        .collect::<VecDeque<_>>();

    doc_csv.push_front(mq_lang::RuntimeValue::String(
        ["Function Name", "Description", "Parameters", "Example"]
            .iter()
            .join("\t"),
    ));

    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();

    let doc_values = engine
        .eval(
            r#"include "csv" | tsv_parse(false) | csv_to_markdown_table()"#,
            mq_lang::raw_input(&doc_csv.iter().join("\n")).into_iter(),
        )
        .map_err(|e| *e)?;

    let mut result = doc_values.values().iter().map(|v| v.to_string()).join("\n");

    if !all_selectors.is_empty() {
        let mut selector_csv = all_selectors
            .iter()
            .map(|[name, description]| {
                mq_lang::RuntimeValue::String([name.as_str(), description.as_str()].into_iter().join("\t"))
            })
            .collect::<VecDeque<_>>();

        selector_csv.push_front(mq_lang::RuntimeValue::String(
            ["Selector", "Description"].iter().join("\t"),
        ));

        let mut engine = mq_lang::DefaultEngine::default();
        engine.load_builtin_module();

        let selector_values = engine
            .eval(
                r#"include "csv" | tsv_parse(false) | csv_to_markdown_table()"#,
                mq_lang::raw_input(&selector_csv.iter().join("\n")).into_iter(),
            )
            .map_err(|e| *e)?;

        result.push_str("\n\n## Selectors\n\n");
        result.push_str(&selector_values.values().iter().map(|v| v.to_string()).join("\n"));
    }

    Ok(result)
}

/// Format documentation as plain text.
fn format_text(module_docs: &[ModuleDoc]) -> String {
    let functions = module_docs
        .iter()
        .flat_map(|m| m.symbols.iter())
        .map(|[name, description, args, _]| {
            let name = name.replace('`', "");
            let args = args.replace('`', "");
            format!("# {description}\ndef {name}({args})")
        })
        .join("\n\n");

    let selectors = module_docs
        .iter()
        .flat_map(|m| m.selectors.iter())
        .map(|[name, description]| {
            let name = name.replace('`', "");
            format!("# {description}\nselector {name}")
        })
        .join("\n\n");

    if selectors.is_empty() {
        functions
    } else {
        format!("{functions}\n\n{selectors}")
    }
}

/// Build HTML table rows for a set of symbols.
fn build_table_rows(symbols: &VecDeque<[String; 4]>) -> String {
    symbols
        .iter()
        .map(|[name, description, args, example]| {
            let name_html = if name.starts_with("~~") {
                let inner = name.trim_start_matches("~~`").trim_end_matches("`~~");
                format!("<del><code>{}</code></del>", escape_html(inner))
            } else {
                let inner = name.trim_start_matches('`').trim_end_matches('`');
                format!("<code>{}</code>", escape_html(inner))
            };
            let args_html = args
                .split(", ")
                .filter(|a| !a.is_empty())
                .map(|a| {
                    let inner = a.trim_start_matches('`').trim_end_matches('`');
                    format!("<code>{}</code>", escape_html(inner))
                })
                .join(", ");
            let desc_html = escape_html(description);
            let example_html = escape_html(example);

            format!(
                "                <tr>\n\
                 \x20                 <td>{name_html}</td>\n\
                 \x20                 <td>{desc_html}</td>\n\
                 \x20                 <td>{args_html}</td>\n\
                 \x20                 <td><code>{example_html}</code></td>\n\
                 \x20               </tr>"
            )
        })
        .join("\n")
}

/// Build HTML table rows for a set of selectors.
fn build_selector_table_rows(selectors: &VecDeque<[String; 2]>) -> String {
    selectors
        .iter()
        .map(|[name, description]| {
            let inner = name.trim_start_matches('`').trim_end_matches('`');
            let name_html = format!("<code>{}</code>", escape_html(inner));
            let desc_html = escape_html(description);

            format!(
                "                <tr>\n\
                 \x20                 <td>{name_html}</td>\n\
                 \x20                 <td>{desc_html}</td>\n\
                 \x20               </tr>"
            )
        })
        .join("\n")
}

/// Build a module page HTML block.
fn build_module_page(id: &str, symbols: &VecDeque<[String; 4]>, active: bool) -> String {
    let rows = build_table_rows(symbols);
    let count = symbols.len();
    let active_class = if active { " active" } else { "" };
    format!(
        "<div class=\"module-page{active_class}\" id=\"{id}\">\n\
         \x20 <div class=\"search-box\">\n\
         \x20   <svg class=\"search-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"11\" cy=\"11\" r=\"8\"/><line x1=\"21\" y1=\"21\" x2=\"16.65\" y2=\"16.65\"/></svg>\n\
         \x20   <input type=\"text\" class=\"search-input\" placeholder=\"Filter functions...\" />\n\
         \x20 </div>\n\
         \x20 <p class=\"count\"><span class=\"count-num\">{count}</span> functions</p>\n\
         \x20 <table>\n\
         \x20   <thead><tr><th>Function</th><th>Description</th><th>Parameters</th><th>Example</th></tr></thead>\n\
         \x20   <tbody>\n{rows}\n\x20   </tbody>\n\
         \x20 </table>\n\
         </div>"
    )
}

/// Build a selector page HTML block.
fn build_selector_page(id: &str, selectors: &VecDeque<[String; 2]>, active: bool) -> String {
    let rows = build_selector_table_rows(selectors);
    let count = selectors.len();
    let active_class = if active { " active" } else { "" };
    format!(
        "<div class=\"module-page{active_class}\" id=\"{id}\">\n\
         \x20 <div class=\"search-box\">\n\
         \x20   <svg class=\"search-icon\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"><circle cx=\"11\" cy=\"11\" r=\"8\"/><line x1=\"21\" y1=\"21\" x2=\"16.65\" y2=\"16.65\"/></svg>\n\
         \x20   <input type=\"text\" class=\"search-input\" placeholder=\"Filter selectors...\" />\n\
         \x20 </div>\n\
         \x20 <p class=\"count\"><span class=\"count-num\">{count}</span> selectors</p>\n\
         \x20 <table>\n\
         \x20   <thead><tr><th>Selector</th><th>Description</th></tr></thead>\n\
         \x20   <tbody>\n{rows}\n\x20   </tbody>\n\
         \x20 </table>\n\
         </div>"
    )
}

/// Format documentation as a single-page HTML with sidebar navigation.
fn format_html(module_docs: &[ModuleDoc]) -> String {
    let has_multiple = module_docs.len() > 1;
    let has_selectors = module_docs.iter().any(|m| !m.selectors.is_empty());

    // Build sidebar items for modules (functions)
    let sidebar_items = if has_multiple {
        let all_count: usize = module_docs.iter().map(|m| m.symbols.len()).sum();
        let all_icon = svg_icon(
            "<rect x=\"3\" y=\"3\" width=\"7\" height=\"7\"/>\
             <rect x=\"14\" y=\"3\" width=\"7\" height=\"7\"/>\
             <rect x=\"3\" y=\"14\" width=\"7\" height=\"7\"/>\
             <rect x=\"14\" y=\"14\" width=\"7\" height=\"7\"/>",
        );
        let mut items = format!(
            "<a class=\"sidebar-link active\" href=\"#\" data-module=\"mod-all\">\
             <span class=\"sidebar-icon\">{all_icon}</span>\
             <span class=\"sidebar-label\">All</span>\
             <span class=\"sidebar-count\">{all_count}</span></a>\n"
        );
        for (i, m) in module_docs.iter().enumerate() {
            let name = escape_html(&m.name);
            let count = m.symbols.len();
            let icon = module_icon(&m.name);
            items.push_str(&format!(
                "<a class=\"sidebar-link\" href=\"#\" data-module=\"mod-{i}\">\
                 <span class=\"sidebar-icon\">{icon}</span>\
                 <span class=\"sidebar-label\">{name}</span>\
                 <span class=\"sidebar-count\">{count}</span></a>\n"
            ));
        }
        items
    } else {
        let m = &module_docs[0];
        let name = escape_html(&m.name);
        let count = m.symbols.len();
        let icon = module_icon(&m.name);
        format!(
            "<a class=\"sidebar-link active\" href=\"#\" data-module=\"mod-all\">\
             <span class=\"sidebar-icon\">{icon}</span>\
             <span class=\"sidebar-label\">{name}</span>\
             <span class=\"sidebar-count\">{count}</span></a>\n"
        )
    };

    // Build sidebar items for selectors
    let selector_sidebar_items = if has_selectors {
        let mut items = String::new();
        for (i, m) in module_docs.iter().enumerate() {
            if m.selectors.is_empty() {
                continue;
            }
            let name = escape_html(&m.name);
            let count = m.selectors.len();
            let icon = selector_icon();
            items.push_str(&format!(
                "<a class=\"sidebar-link\" href=\"#\" data-module=\"sel-{i}\">\
                 <span class=\"sidebar-icon\">{icon}</span>\
                 <span class=\"sidebar-label\">{name}</span>\
                 <span class=\"sidebar-count\">{count}</span></a>\n"
            ));
        }
        items
    } else {
        String::new()
    };

    // Build function pages
    let mut pages = if has_multiple {
        let all_symbols: VecDeque<_> = module_docs.iter().flat_map(|m| m.symbols.iter()).cloned().collect();
        let mut pages_html = build_module_page("mod-all", &all_symbols, true);
        for (i, m) in module_docs.iter().enumerate() {
            pages_html.push('\n');
            pages_html.push_str(&build_module_page(&format!("mod-{i}"), &m.symbols, false));
        }
        pages_html
    } else {
        build_module_page("mod-all", &module_docs[0].symbols, true)
    };

    // Build selector pages
    if has_selectors {
        for (i, m) in module_docs.iter().enumerate() {
            if m.selectors.is_empty() {
                continue;
            }
            pages.push('\n');
            pages.push_str(&build_selector_page(&format!("sel-{i}"), &m.selectors, false));
        }
    }

    // Build selector sidebar section
    let selector_section = if has_selectors {
        format!(
            "        <nav class=\"sidebar-section\">\n\
             \x20         <div class=\"sidebar-section-title\">Selectors</div>\n\
             {selector_sidebar_items}\
             \x20       </nav>\n"
        )
    } else {
        String::new()
    };

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>mq - Function Reference</title>
    <link rel="preconnect" href="https://fonts.googleapis.com" />
    <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
    <link href="https://fonts.googleapis.com/css2?family=Montserrat:wght@400;500;600;700&display=swap" rel="stylesheet" />
    <style>
      :root {{
        --bg-primary: #2a3444;
        --bg-secondary: #232d3b;
        --bg-tertiary: #3d4a5c;
        --text-primary: #e2e8f0;
        --text-secondary: #cbd5e1;
        --text-muted: #94a3b8;
        --accent-primary: #67b8e3;
        --accent-secondary: #4fc3f7;
        --border-default: #4a5568;
        --border-muted: #374151;
        --code-bg: #1e293b;
        --code-bg-inline: #374151;
        --code-color: #e2e8f0;
        --sidebar-width: 260px;
      }}

      * {{ margin: 0; padding: 0; box-sizing: border-box; }}
      html {{ height: 100%; scroll-behavior: smooth; }}

      body {{
        background-color: var(--bg-primary);
        color: var(--text-primary);
        font-family: "Montserrat", -apple-system, BlinkMacSystemFont, "Segoe UI", "Noto Sans", Helvetica, Arial, sans-serif;
        font-weight: 400;
        line-height: 1.6;
        min-height: 100vh;
      }}

      /* ---- Layout ---- */
      .layout {{
        display: flex;
        min-height: 100vh;
      }}

      .sidebar {{
        background-color: var(--bg-secondary);
        border-right: 1px solid var(--border-default);
        display: flex;
        flex-direction: column;
        height: 100vh;
        overflow-y: auto;
        position: fixed;
        top: 0;
        left: 0;
        width: var(--sidebar-width);
        z-index: 50;
      }}

      .sidebar-header {{
        border-bottom: 1px solid var(--border-default);
        padding: 1.25rem 1.25rem 1rem;
      }}

      .sidebar-header h1 {{
        color: var(--accent-primary);
        font-size: 1.3rem;
        font-weight: 700;
        letter-spacing: -0.3px;
      }}

      .sidebar-header p {{
        color: var(--text-muted);
        font-size: 0.75rem;
        margin-top: 0.2rem;
      }}

      .sidebar-section {{
        padding: 0.75rem 0;
      }}

      .sidebar-section-title {{
        color: var(--text-muted);
        font-size: 0.7rem;
        font-weight: 600;
        letter-spacing: 0.8px;
        padding: 0.25rem 1.25rem 0.5rem;
        text-transform: uppercase;
      }}

      .sidebar-link {{
        align-items: center;
        border-left: 3px solid transparent;
        color: var(--text-secondary);
        cursor: pointer;
        display: flex;
        font-size: 0.85rem;
        gap: 0.6rem;
        padding: 0.5rem 1.25rem;
        text-decoration: none;
        transition: all 0.15s;
      }}

      .sidebar-link:hover {{
        background-color: rgba(103, 184, 227, 0.06);
        color: var(--text-primary);
      }}

      .sidebar-link.active {{
        background-color: rgba(103, 184, 227, 0.1);
        border-left-color: var(--accent-primary);
        color: var(--accent-primary);
        font-weight: 600;
      }}

      .sidebar-icon {{
        display: flex;
        align-items: center;
        flex-shrink: 0;
      }}

      .sidebar-icon svg {{
        height: 16px;
        width: 16px;
      }}

      .sidebar-label {{
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
      }}

      .sidebar-count {{
        background-color: var(--bg-tertiary);
        border-radius: 10px;
        color: var(--text-muted);
        font-size: 0.7rem;
        font-weight: 600;
        min-width: 1.6rem;
        padding: 0.1rem 0.45rem;
        text-align: center;
      }}

      .sidebar-link.active .sidebar-count {{
        background-color: rgba(103, 184, 227, 0.15);
        color: var(--accent-primary);
      }}

      .content {{
        flex: 1;
        margin-left: var(--sidebar-width);
        padding: 2rem 2.5rem;
        max-width: calc(100% - var(--sidebar-width));
      }}

      /* ---- Mobile sidebar toggle ---- */
      .sidebar-toggle {{
        background-color: var(--bg-tertiary);
        border: 1px solid var(--border-default);
        border-radius: 8px;
        color: var(--text-primary);
        cursor: pointer;
        display: none;
        left: 1rem;
        padding: 0.5rem;
        position: fixed;
        top: 1rem;
        z-index: 60;
      }}

      .sidebar-toggle svg {{
        display: block;
        height: 20px;
        width: 20px;
      }}

      .sidebar-overlay {{
        background-color: rgba(0, 0, 0, 0.5);
        display: none;
        inset: 0;
        position: fixed;
        z-index: 40;
      }}

      /* ---- Pages ---- */
      .module-page {{ display: none; }}
      .module-page.active {{ display: block; }}

      .page-title {{
        color: var(--text-primary);
        font-size: 1.5rem;
        font-weight: 700;
        margin-bottom: 1.5rem;
      }}

      .search-box {{
        margin-bottom: 1.5rem;
        position: relative;
      }}

      .search-box input {{
        background-color: var(--bg-tertiary);
        border: 1px solid var(--border-default);
        border-radius: 8px;
        color: var(--text-primary);
        font-family: inherit;
        font-size: 0.95rem;
        padding: 0.75rem 1rem 0.75rem 2.5rem;
        width: 100%;
        transition: border-color 0.2s;
      }}

      .search-box input:focus {{
        border-color: var(--accent-primary);
        outline: none;
      }}

      .search-box .search-icon {{
        color: var(--text-muted);
        height: 16px;
        left: 0.85rem;
        pointer-events: none;
        position: absolute;
        top: 50%;
        transform: translateY(-50%);
        width: 16px;
      }}

      .count {{
        color: var(--text-muted);
        font-size: 0.85rem;
        margin-bottom: 1rem;
      }}

      table {{ border-collapse: collapse; width: 100%; }}

      thead th {{
        background-color: var(--bg-tertiary);
        border-bottom: 2px solid var(--accent-primary);
        color: var(--accent-primary);
        font-size: 0.8rem;
        font-weight: 600;
        letter-spacing: 0.5px;
        padding: 0.75rem 1rem;
        position: sticky;
        text-align: left;
        text-transform: uppercase;
        top: 0;
        z-index: 5;
      }}

      tbody tr {{
        border-bottom: 1px solid var(--border-muted);
        cursor: pointer;
        transition: background-color 0.15s;
      }}

      tbody tr:hover {{ background-color: var(--bg-tertiary); }}

      tbody td {{
        font-size: 0.9rem;
        padding: 0.65rem 1rem;
        vertical-align: top;
      }}

      tbody td:first-child {{ white-space: nowrap; }}

      code {{
        background-color: var(--code-bg-inline);
        border-radius: 4px;
        color: var(--code-color);
        font-family: "Consolas", "Monaco", "Courier New", monospace;
        font-size: 0.85em;
        padding: 0.15em 0.4em;
      }}

      del code {{ opacity: 0.6; }}

      footer {{
        border-top: 1px solid var(--border-default);
        margin-left: var(--sidebar-width);
        padding: 1.5rem 2.5rem;
      }}

      footer p {{
        color: var(--text-muted);
        font-size: 0.85rem;
      }}

      footer a {{
        color: var(--accent-primary);
        text-decoration: none;
      }}

      footer a:hover {{
        color: var(--accent-secondary);
        text-decoration: underline;
      }}

      @media (max-width: 768px) {{
        .sidebar {{
          transform: translateX(-100%);
          transition: transform 0.25s ease;
        }}

        .sidebar.open {{
          transform: translateX(0);
        }}

        .sidebar-toggle {{
          display: block;
        }}

        .sidebar-overlay.open {{
          display: block;
        }}

        .content {{
          margin-left: 0;
          max-width: 100%;
          padding: 1.5rem 1rem;
          padding-top: 4rem;
        }}

        footer {{
          margin-left: 0;
          padding: 1.5rem 1rem;
        }}

        table {{ display: block; overflow-x: auto; }}

        tbody td, thead th {{
          font-size: 0.8rem;
          padding: 0.6rem 0.75rem;
        }}
      }}
    </style>
  </head>
  <body>
    <button class="sidebar-toggle" id="sidebarToggle">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
        <line x1="3" y1="12" x2="21" y2="12"/><line x1="3" y1="6" x2="21" y2="6"/><line x1="3" y1="18" x2="21" y2="18"/>
      </svg>
    </button>
    <div class="sidebar-overlay" id="sidebarOverlay"></div>

    <div class="layout">
      <aside class="sidebar" id="sidebar">
        <div class="sidebar-header">
          <h1>mq</h1>
          <p>Function Reference</p>
        </div>
        <nav class="sidebar-section">
          <div class="sidebar-section-title">Modules</div>
{sidebar_items}
        </nav>
{selector_section}
      </aside>

      <div class="content">
{pages}
      </div>
    </div>

    <footer>
      <p>Generated by <a href="https://github.com/harehare/mq">mq</a></p>
    </footer>

    <script>
      // Sidebar navigation
      document.querySelectorAll(".sidebar-link").forEach(function (link) {{
        link.addEventListener("click", function (e) {{
          e.preventDefault();
          document.querySelectorAll(".sidebar-link").forEach(function (l) {{
            l.classList.remove("active");
          }});
          link.classList.add("active");

          var target = link.getAttribute("data-module");
          document.querySelectorAll(".module-page").forEach(function (page) {{
            page.classList.toggle("active", page.id === target);
          }});

          // Close mobile sidebar
          document.getElementById("sidebar").classList.remove("open");
          document.getElementById("sidebarOverlay").classList.remove("open");
        }});
      }});

      // Search filter
      document.querySelectorAll(".search-input").forEach(function (input) {{
        input.addEventListener("input", function () {{
          var page = input.closest(".module-page");
          var q = input.value.toLowerCase();
          var rows = page.querySelectorAll("tbody tr");
          var visible = 0;
          rows.forEach(function (row) {{
            var text = row.textContent.toLowerCase();
            var show = text.includes(q);
            row.style.display = show ? "" : "none";
            if (show) visible++;
          }});
          page.querySelector(".count-num").textContent = visible;
        }});
      }});

      // Mobile sidebar toggle
      document.getElementById("sidebarToggle").addEventListener("click", function () {{
        document.getElementById("sidebar").classList.toggle("open");
        document.getElementById("sidebarOverlay").classList.toggle("open");
      }});
      document.getElementById("sidebarOverlay").addEventListener("click", function () {{
        document.getElementById("sidebar").classList.remove("open");
        document.getElementById("sidebarOverlay").classList.remove("open");
      }});
    </script>
  </body>
</html>"#,
    )
}

/// Generate an inline SVG icon with the given inner elements.
fn svg_icon(inner: &str) -> String {
    format!(
        "<svg viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" \
         stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{inner}</svg>"
    )
}

/// Return an appropriate SVG icon for a module name.
fn module_icon(name: &str) -> String {
    if name.starts_with("Built-in") {
        // cube icon
        svg_icon(
            "<path d=\"M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z\"/>\
             <polyline points=\"3.27 6.96 12 12.01 20.73 6.96\"/>\
             <line x1=\"12\" y1=\"22.08\" x2=\"12\" y2=\"12\"/>",
        )
    } else {
        // package icon
        svg_icon(
            "<line x1=\"16.5\" y1=\"9.4\" x2=\"7.5\" y2=\"4.21\"/>\
             <path d=\"M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z\"/>\
             <polyline points=\"3.27 6.96 12 12.01 20.73 6.96\"/>\
             <line x1=\"12\" y1=\"22.08\" x2=\"12\" y2=\"12\"/>",
        )
    }
}

/// Return an SVG icon for selector items.
fn selector_icon() -> String {
    // crosshair/target icon
    svg_icon(
        "<circle cx=\"12\" cy=\"12\" r=\"10\"/>\
         <line x1=\"22\" y1=\"12\" x2=\"18\" y2=\"12\"/>\
         <line x1=\"6\" y1=\"12\" x2=\"2\" y2=\"12\"/>\
         <line x1=\"12\" y1=\"6\" x2=\"12\" y2=\"2\"/>\
         <line x1=\"12\" y1=\"22\" x2=\"12\" y2=\"18\"/>",
    )
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
