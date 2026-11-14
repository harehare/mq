use std::sync::{Arc, RwLock};

use url::Url;

/// `lib.mq` defines `add` (also a builtin name) and `exported`.
pub(crate) const LIB: &str = "# Adds numbers\ndef add(a, b): a + b;\nlet exported = 5";

/// A HIR over a temporary module directory holding `lib.mq`, with `code` opened as `main.mq`.
pub(crate) fn hir_with_lib(name: &str, code: &str) -> (Arc<RwLock<mq_hir::Hir>>, Url) {
    let dir = std::env::temp_dir().join(format!("mq-lsp-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("lib.mq"), LIB).unwrap();

    let mut loader = mq_lang::ModuleLoader::default();
    loader.set_search_paths(vec![dir.clone()]);
    let mut hir = mq_hir::Hir::new(loader);
    hir.add_builtin();

    let url = Url::from_file_path(dir.join("main.mq")).unwrap();
    hir.add_code(Some(url.clone()), code);
    (Arc::new(RwLock::new(hir)), url)
}
