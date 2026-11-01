use std::{path::Path, str::FromStr};

use tower_lsp_server::ls_types::Uri;
use url::Url;

/// URI scheme for the built-in sources that have no file on disk.
pub(crate) const SCHEME: &str = "mq-std";
const BUILTIN: &str = "builtin";

/// Returns the URI to show for `source_id`: a virtual document for `builtin.mq` and the
/// standard modules, or the source's own URL.
pub(crate) fn uri_for_source(hir: &mq_hir::Hir, source_id: mq_hir::SourceId) -> Option<Uri> {
    if source_id == hir.builtin.source_id {
        return virtual_uri(BUILTIN);
    }
    let url = hir.url_by_source(&source_id)?;
    match standard_module_name(url) {
        Some(name) => virtual_uri(name),
        None => Uri::from_str(url.as_ref()).ok(),
    }
}

/// Returns true if `uri` points at a document that can be opened.
pub(crate) fn is_openable(uri: &Uri) -> bool {
    match Url::parse(uri.as_str()) {
        Ok(url) if url.scheme() == "file" => Path::new(url.path()).exists(),
        Ok(_) => true,
        Err(_) => false,
    }
}

/// Returns the text of a virtual document.
pub(crate) fn content(uri: &str) -> Option<String> {
    let url = Url::parse(uri).ok()?;
    if url.scheme() != SCHEME {
        return None;
    }
    let name = url.path().strip_prefix('/')?.strip_suffix(".mq")?;
    if name == BUILTIN {
        return Some(mq_lang::BUILTIN_MODULE_FILE.to_string());
    }
    mq_lang::STANDARD_MODULES.get(name).map(|source| source().to_string())
}

fn virtual_uri(name: &str) -> Option<Uri> {
    Uri::from_str(&format!("{SCHEME}:/{name}.mq")).ok()
}

/// Standard modules are registered under a placeholder `file:///<name>` URL.
fn standard_module_name(url: &Url) -> Option<&str> {
    if url.scheme() != "file" || Path::new(url.path()).exists() {
        return None;
    }
    let name = url.path().strip_prefix('/')?;
    mq_lang::STANDARD_MODULES.contains_key(name).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_of_builtin_and_standard_modules() {
        assert_eq!(
            content("mq-std:/builtin.mq").as_deref(),
            Some(mq_lang::BUILTIN_MODULE_FILE)
        );
        assert!(content("mq-std:/section.mq").is_some_and(|text| text.contains("def ")));
        assert!(content("mq-std:/nope.mq").is_none());
        assert!(content("file:///section.mq").is_none());
    }

    #[test]
    fn test_uri_for_source_maps_builtin_and_standard_modules() {
        let mut hir = mq_hir::Hir::default();
        hir.add_builtin();
        let (module, _, _) = hir.add_code(Some(Url::parse("file:///section").unwrap()), "def f(): 1;");
        let (local, _, _) = hir.add_code(Some(Url::parse("file:///work/a.mq").unwrap()), "1");

        let uri = |id| uri_for_source(&hir, id).map(|u| u.to_string());
        assert_eq!(uri(hir.builtin.source_id).as_deref(), Some("mq-std:/builtin.mq"));
        assert_eq!(uri(module).as_deref(), Some("mq-std:/section.mq"));
        assert_eq!(uri(local).as_deref(), Some("file:///work/a.mq"));
    }
}
