//! `mq module`: copy HTTP modules into a local module directory.
use super::Cli;
use miette::{IntoDiagnostic, WrapErr, bail, miette};
use mq_lang::{HttpModuleResolver, LOCKFILE_NAME, ModuleLock, ModuleResolver, UreqFetcher, compute_hash, http_import};
use std::fs;
use std::path::{Path, PathBuf};

/// Where vendored modules and their lock file live.
pub(super) struct Target {
    pub(super) dir: PathBuf,
    pub(super) lockfile: PathBuf,
    /// Module download cache; the user cache directory when `None`.
    pub(super) cache_dir: Option<PathBuf>,
}

impl Target {
    fn project() -> Self {
        Self {
            dir: PathBuf::from("vendor"),
            lockfile: PathBuf::from(LOCKFILE_NAME),
            cache_dir: None,
        }
    }

    /// `$HOME/.config/mq`, one of the default module search directories.
    fn global() -> miette::Result<Self> {
        let dir = dirs::home_dir()
            .ok_or_else(|| miette!("Could not determine home directory"))?
            .join(".config")
            .join("mq");
        Ok(Self {
            lockfile: dir.join(LOCKFILE_NAME),
            cache_dir: None,
            dir,
        })
    }

    pub(super) fn select(global: bool) -> miette::Result<Self> {
        if global { Self::global() } else { Ok(Self::project()) }
    }

    pub(super) fn create_dir(&self) -> miette::Result<()> {
        fs::create_dir_all(&self.dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to create {}", self.dir.display()))
    }

    fn module_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.mq"))
    }
}

/// The file stem a module is stored under, e.g. `kdl` for `github.com/harehare/kdl.mq@v1.0`.
pub(super) fn module_name(spec: &str) -> miette::Result<&str> {
    let name = http_import::extract_module_name(spec);
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        bail!("Cannot derive a module name from `{spec}`");
    }
    Ok(name)
}

/// Locked entries as `(url, name, hash)`, rejecting two different contents stored under one name.
pub(super) fn locked_modules(lock: &ModuleLock) -> miette::Result<Vec<(String, String, String)>> {
    let mut modules: Vec<(String, String, String)> = Vec::new();
    for (url, hash) in lock.entries() {
        let name = module_name(url)?.to_string();
        if let Some((other, _, other_hash)) = modules.iter().find(|(_, n, _)| *n == name)
            && other_hash != hash
        {
            bail!("{other} and {url} would both be vendored as `{name}.mq`");
        }
        modules.push((url.to_string(), name, hash.to_string()));
    }
    Ok(modules)
}

/// Compares vendored files with `lock`, returning one message per problem.
fn check_modules(target: &Target, lock: &ModuleLock) -> miette::Result<Vec<String>> {
    let mut problems = Vec::new();
    for (url, name, hash) in locked_modules(lock)? {
        let path = target.module_path(&name);
        match fs::read_to_string(&path) {
            Ok(content) if compute_hash(&content) == hash => {}
            Ok(_) => problems.push(format!("{} does not match the locked content of {url}", path.display())),
            Err(_) => problems.push(format!("{} is missing (locked as {url})", path.display())),
        }
    }
    Ok(problems)
}

pub(super) fn read_lock(path: &Path) -> miette::Result<ModuleLock> {
    let content = fs::read_to_string(path)
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to read {}", path.display()))?;
    ModuleLock::parse(&content).map_err(|e| miette!("Invalid {}: {e}", path.display()))
}

/// Drops lock entries that map to `name` but hold different content than `hash`, so upgrading
/// a module does not leave its old version behind.
fn prune_replaced(lockfile: &Path, name: &str, hash: &str) -> miette::Result<()> {
    let mut lock = read_lock(lockfile)?;
    let stale: Vec<String> = lock
        .entries()
        .filter(|(url, h)| *h != hash && module_name(url).is_ok_and(|n| n == name))
        .map(|(url, _)| url.to_string())
        .collect();
    if stale.is_empty() {
        return Ok(());
    }
    for url in &stale {
        lock.remove(url);
    }
    write_lock(lockfile, &lock)
}

fn write_lock(path: &Path, lock: &ModuleLock) -> miette::Result<()> {
    fs::write(path, lock.to_json())
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to write {}", path.display()))
}

/// Drops the lock entry for `url`.
pub(super) fn remove_lock_entry(lockfile: &Path, url: &str) -> miette::Result<()> {
    let mut lock = read_lock(lockfile)?;
    lock.remove(url);
    write_lock(lockfile, &lock)
}

impl Cli {
    /// `mq module add [--global] MODULE...`
    pub(super) fn module_add(specs: &[String], global: bool, domains: &[String]) -> miette::Result<()> {
        Self::add_modules(&Target::select(global)?, specs, domains)
    }

    fn add_modules(target: &Target, specs: &[String], domains: &[String]) -> miette::Result<()> {
        target.create_dir()?;
        for spec in specs {
            if !(http_import::is_github_url(spec) || http_import::is_remote_url(spec)) {
                bail!("`{spec}` is not a github.com/OWNER/REPO[@VERSION] or https:// module");
            }
            let name = module_name(spec)?;
            // A fresh resolver per module: its cached lock must not outlive `prune_replaced`.
            let resolver = Self::module_resolver(target, domains, false);
            let hash = Self::fetch_module(&resolver, target, spec, name)?;
            prune_replaced(&target.lockfile, name, &hash)?;
        }
        Ok(())
    }

    /// `mq module install [--global]`
    pub(super) fn module_install(global: bool, domains: &[String]) -> miette::Result<()> {
        let target = Target::select(global)?;
        let modules = locked_modules(&read_lock(&target.lockfile)?)?;
        if modules.is_empty() {
            bail!("{} has no modules to install", target.lockfile.display());
        }
        target.create_dir()?;
        // The lock file already pins every URL, so an install must never record new content.
        let resolver = Self::module_resolver(&target, domains, true);
        for (url, name, _) in modules {
            Self::fetch_module(&resolver, &target, &url, &name)?;
        }
        Ok(())
    }

    /// `mq module check [--global]`
    pub(super) fn module_check(global: bool) -> miette::Result<()> {
        let target = Target::select(global)?;
        let problems = check_modules(&target, &read_lock(&target.lockfile)?)?;
        if problems.is_empty() {
            return Ok(());
        }
        for problem in &problems {
            eprintln!("{problem}");
        }
        bail!("{} module(s) are out of date", problems.len());
    }

    pub(super) fn module_resolver(
        target: &Target,
        domains: &[String],
        frozen: bool,
    ) -> HttpModuleResolver<UreqFetcher> {
        let mut fetcher = UreqFetcher::default();
        fetcher.set_lockfile_path(target.lockfile.clone());
        fetcher.set_lockfile_frozen(frozen);
        if let Some(dir) = &target.cache_dir {
            fetcher.set_cache_dir(dir.clone());
        }
        let mut resolver = HttpModuleResolver::new(domains.to_vec(), fetcher);
        resolver.set_enabled(true);
        resolver
    }

    /// Fetches `spec` through the lock-checked resolver and writes it as `<name>.mq`, returning its hash.
    pub(super) fn fetch_module(
        resolver: &HttpModuleResolver<UreqFetcher>,
        target: &Target,
        spec: &str,
        name: &str,
    ) -> miette::Result<String> {
        let content = resolver.resolve(spec).map_err(|e| miette!(e.to_string()))?;
        let path = target.module_path(name);
        fs::write(&path, &content)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to write {}", path.display()))?;
        println!("{}", path.display());
        Ok(compute_hash(&content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case("github.com/harehare/kdl.mq@v1.0", Some("kdl"))]
    #[case("github.com/harehare/kdl.mq", Some("kdl"))]
    #[case("https://raw.githubusercontent.com/harehare/kdl.mq/v1.0/kdl.mq", Some("kdl"))]
    #[case("https://example.com/", None)]
    #[case("https://example.com/..mq", None)]
    fn test_module_name(#[case] spec: &str, #[case] expected: Option<&str>) {
        assert_eq!(module_name(spec).ok(), expected);
    }

    fn lock_with(entries: &[(&str, &str)]) -> ModuleLock {
        let mut lock = ModuleLock::default();
        for (url, hash) in entries {
            lock.insert(*url, *hash);
        }
        lock
    }

    #[test]
    fn test_locked_modules_rejects_name_collision() {
        let lock = lock_with(&[
            ("https://raw.githubusercontent.com/harehare/kdl.mq/v1.0/kdl.mq", "aa"),
            ("https://raw.githubusercontent.com/harehare/kdl.mq/v2.0/kdl.mq", "bb"),
        ]);
        assert!(locked_modules(&lock).is_err());
    }

    #[test]
    fn test_check_modules_reports_missing_and_changed() {
        let dir = tempfile::tempdir().unwrap();
        let target = Target {
            dir: dir.path().to_path_buf(),
            lockfile: dir.path().join(LOCKFILE_NAME),
            cache_dir: None,
        };
        fs::write(target.module_path("ok"), "def a(): 1;").unwrap();
        fs::write(target.module_path("changed"), "def a(): 2;").unwrap();
        let lock = lock_with(&[
            (
                "https://raw.githubusercontent.com/harehare/ok.mq/v1/ok.mq",
                &compute_hash("def a(): 1;"),
            ),
            (
                "https://raw.githubusercontent.com/harehare/changed.mq/v1/changed.mq",
                "00",
            ),
            ("https://raw.githubusercontent.com/harehare/gone.mq/v1/gone.mq", "00"),
        ]);

        let problems = check_modules(&target, &lock).unwrap();

        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn test_prune_replaced_keeps_current_version_only() {
        let dir = tempfile::tempdir().unwrap();
        let lockfile = dir.path().join(LOCKFILE_NAME);
        let old = "https://raw.githubusercontent.com/harehare/kdl.mq/v1.0/kdl.mq";
        let new = "https://raw.githubusercontent.com/harehare/kdl.mq/v2.0/kdl.mq";
        fs::write(&lockfile, lock_with(&[(old, "aa"), (new, "bb")]).to_json()).unwrap();

        prune_replaced(&lockfile, "kdl", "bb").unwrap();

        let lock = read_lock(&lockfile).unwrap();
        assert_eq!(lock.entries().map(|(u, _)| u).collect::<Vec<_>>(), vec![new]);
    }

    const KDL_V1: &str = "https://raw.githubusercontent.com/harehare/kdl.mq/v1.0/kdl.mq";
    const KDL_V2: &str = "https://raw.githubusercontent.com/harehare/kdl.mq/v2.0/kdl.mq";
    const INI_V1: &str = "https://raw.githubusercontent.com/harehare/ini.mq/v1.0/ini.mq";
    const OTHER_HOST: &str = "https://example.invalid/mods/kdl.mq";
    const INI_V2: &str = "https://raw.githubusercontent.com/harehare/ini.mq/v2.0/ini.mq";

    /// A project layout in a temp dir whose module cache is pre-seeded, so nothing hits the network.
    struct Fixture {
        _dir: tempfile::TempDir,
        target: Target,
        domains: Vec<String>,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let target = Target {
                dir: dir.path().join("vendor"),
                lockfile: dir.path().join(LOCKFILE_NAME),
                cache_dir: Some(dir.path().join("cache")),
            };
            Self {
                _dir: dir,
                target,
                domains: vec!["github.com/harehare".to_string()],
            }
        }

        /// Makes `url` resolvable offline with `content`.
        fn seed(&self, url: &str, content: &str) {
            let kind = if http_import::is_versioned_url(url) {
                "versioned"
            } else {
                "mutable"
            };
            let dir = self.target.cache_dir.as_ref().unwrap().join(kind);
            fs::create_dir_all(&dir).unwrap();
            let stem = format!("{:x}", md5::compute(url));
            fs::write(dir.join(format!("{stem}.mq")), content).unwrap();
            fs::write(dir.join(format!("{stem}.mq.sha256")), compute_hash(content)).unwrap();
        }

        fn pin(&self, entries: &[(&str, &str)]) {
            let entries: Vec<_> = entries.iter().map(|(u, c)| (*u, compute_hash(c))).collect();
            let entries: Vec<_> = entries.iter().map(|(u, h)| (*u, h.as_str())).collect();
            fs::write(&self.target.lockfile, lock_with(&entries).to_json()).unwrap();
        }

        fn add(&self, specs: &[&str]) -> miette::Result<()> {
            let specs: Vec<String> = specs.iter().map(|s| s.to_string()).collect();
            Cli::add_modules(&self.target, &specs, &self.domains)
        }

        fn locked_urls(&self) -> Vec<String> {
            let mut urls: Vec<_> = read_lock(&self.target.lockfile)
                .unwrap()
                .entries()
                .map(|(u, _)| u.to_string())
                .collect();
            urls.sort();
            urls
        }

        fn vendored(&self, name: &str) -> Option<String> {
            fs::read_to_string(self.target.module_path(name)).ok()
        }
    }

    #[test]
    fn test_add_vendors_and_locks_a_module() {
        let fx = Fixture::new();
        fx.seed(KDL_V1, "def kdl(): 1;");

        fx.add(&["github.com/harehare/kdl.mq@v1.0"]).unwrap();

        assert_eq!(fx.vendored("kdl").as_deref(), Some("def kdl(): 1;"));
        assert_eq!(fx.locked_urls(), vec![KDL_V1]);
    }

    #[test]
    fn test_add_several_new_modules_locks_each() {
        let fx = Fixture::new();
        fx.seed(KDL_V1, "def kdl(): 1;");
        fx.seed(INI_V1, "def ini(): 1;");

        fx.add(&["github.com/harehare/kdl.mq@v1.0", "github.com/harehare/ini.mq@v1.0"])
            .unwrap();

        assert_eq!(fx.locked_urls(), vec![INI_V1, KDL_V1]);
        assert!(fx.vendored("kdl").is_some() && fx.vendored("ini").is_some());
    }

    #[test]
    fn test_add_upgrading_several_pinned_modules_leaves_one_entry_each() {
        let fx = Fixture::new();
        fx.pin(&[(KDL_V1, "def kdl(): 1;"), (INI_V1, "def ini(): 1;")]);
        fx.seed(KDL_V2, "def kdl(): 2;");
        fx.seed(INI_V2, "def ini(): 2;");

        fx.add(&["github.com/harehare/kdl.mq@v2.0", "github.com/harehare/ini.mq@v2.0"])
            .unwrap();

        assert_eq!(fx.locked_urls(), vec![INI_V2, KDL_V2]);
        assert!(locked_modules(&read_lock(&fx.target.lockfile).unwrap()).is_ok());
        assert_eq!(fx.vendored("kdl").as_deref(), Some("def kdl(): 2;"));
        assert_eq!(fx.vendored("ini").as_deref(), Some("def ini(): 2;"));
    }

    #[test]
    fn test_add_upgrade_keeps_unrelated_pins() {
        let fx = Fixture::new();
        fx.pin(&[(KDL_V1, "def kdl(): 1;"), (INI_V1, "def ini(): 1;")]);
        fx.seed(KDL_V2, "def kdl(): 2;");

        fx.add(&["github.com/harehare/kdl.mq@v2.0"]).unwrap();

        assert_eq!(fx.locked_urls(), vec![INI_V1, KDL_V2]);
    }

    #[test]
    fn test_add_is_idempotent() {
        let fx = Fixture::new();
        fx.seed(KDL_V1, "def kdl(): 1;");

        fx.add(&["github.com/harehare/kdl.mq@v1.0"]).unwrap();
        fx.add(&["github.com/harehare/kdl.mq@v1.0"]).unwrap();

        assert_eq!(fx.locked_urls(), vec![KDL_V1]);
    }

    #[test]
    fn test_add_rejects_content_that_differs_from_the_lock() {
        let fx = Fixture::new();
        fx.pin(&[(KDL_V1, "def kdl(): 1;")]);
        fx.seed(KDL_V1, "def kdl(): tampered;");

        assert!(fx.add(&["github.com/harehare/kdl.mq@v1.0"]).is_err());

        assert_eq!(fx.vendored("kdl"), None);
        assert_eq!(fx.locked_urls(), vec![KDL_V1]);
    }

    #[test]
    fn test_add_rejects_a_local_path() {
        let fx = Fixture::new();

        assert!(fx.add(&["./kdl.mq"]).is_err());

        assert!(!fx.target.lockfile.exists());
    }

    #[test]
    fn test_add_keeps_earlier_modules_when_a_later_spec_fails() {
        let fx = Fixture::new();
        fx.seed(KDL_V1, "def kdl(): 1;");

        assert!(fx.add(&["github.com/harehare/kdl.mq@v1.0", "./nope.mq"]).is_err());

        assert_eq!(fx.locked_urls(), vec![KDL_V1]);
        assert!(fx.vendored("kdl").is_some());
    }

    #[test]
    fn test_add_rejects_a_domain_outside_the_allowlist() {
        let fx = Fixture::new();
        fx.seed(OTHER_HOST, "def kdl(): 1;");

        assert!(fx.add(&[OTHER_HOST]).is_err());

        assert_eq!(fx.vendored("kdl"), None);
        assert!(!fx.target.lockfile.exists());
    }

    #[test]
    fn test_replace_module_across_several_modules_leaves_one_entry_each() {
        let fx = Fixture::new();
        fx.pin(&[(KDL_V1, "def kdl(): 1;"), (INI_V1, "def ini(): 1;")]);
        fx.seed(KDL_V2, "def kdl(): 2;");
        fx.seed(INI_V2, "def ini(): 2;");

        Cli::replace_module(&fx.target, &fx.domains, KDL_V1, KDL_V2, "kdl").unwrap();
        Cli::replace_module(&fx.target, &fx.domains, INI_V1, INI_V2, "ini").unwrap();

        assert_eq!(fx.locked_urls(), vec![INI_V2, KDL_V2]);
        assert!(locked_modules(&read_lock(&fx.target.lockfile).unwrap()).is_ok());
        assert_eq!(fx.vendored("kdl").as_deref(), Some("def kdl(): 2;"));
        assert_eq!(fx.vendored("ini").as_deref(), Some("def ini(): 2;"));
    }

    #[test]
    fn test_replace_module_keeps_the_old_pin_when_the_fetch_fails() {
        let fx = Fixture::new();
        fx.pin(&[(KDL_V1, "def kdl(): 1;")]);
        fx.seed(OTHER_HOST, "def kdl(): 2;");

        assert!(Cli::replace_module(&fx.target, &fx.domains, KDL_V1, OTHER_HOST, "kdl").is_err());

        assert_eq!(fx.locked_urls(), vec![KDL_V1]);
    }
}
