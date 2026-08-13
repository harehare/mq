//! Shared building blocks for mq-test's self-contained HTML reports (coverage, snapshot diffs).

/// Escapes `&`, `<`, `>`, and `"` for safe embedding in HTML.
pub(crate) fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Wraps `body` in a minimal, self-contained HTML page with an inline `<style>` block, so the
/// report can be opened directly as a standalone file. `title` is escaped automatically.
pub(crate) fn page(title: &str, style: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n\
        <html lang=\"en\">\n\
        <head>\n\
        \x20 <meta charset=\"utf-8\">\n\
        \x20 <title>{title}</title>\n\
        \x20 <style>{style}</style>\n\
        </head>\n\
        <body>\n\
        {body}\
        </body>\n\
        </html>\n",
        title = escape(title),
    )
}
