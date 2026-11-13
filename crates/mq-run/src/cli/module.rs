//! `mq module`: copy HTTP modules into a local module directory.
use super::Cli;
use miette::{IntoDiagnostic, WrapErr, bail, miette};
use mq_lang::{HttpModuleResolver, LOCKFILE_NAME, ModuleLock, ModuleResolver, UreqFetcher, compute_hash, http_import};
use std::fs;
use std::path::{Path, PathBuf};

/// Where vendored modules and their lock file live.
struct Target {
    dir: PathBuf,
    lockfile: PathBuf,
}

impl Target {
    fn project() -> Self {
        Self {
            dir: PathBuf::from("vendor"),
            lockfile: PathBuf::from(LOCKFILE_NAME),
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
            dir,
        })
    }

    fn select(global: bool) -> miette::Result<Self> {
        if global { Self::global() } else { Ok(Self::project()) }
    }

    fn create_dir(&self) -> miette::Result<()> {
        fs::create_dir_all(&self.dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to create {}", self.dir.display()))
    }

    fn module_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.mq"))
    }
}

/// The file stem a module is stored under, e.g. `kdl` for `github.com/harehare/kdl.mq@v1.0`.
fn module_name(spec: &str) -> miette::Result<&str> {
    let name = http_import::extract_module_name(spec);
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\']) {
        bail!("Cannot derive a module name from `{spec}`");
    }
    Ok(name)
}

/// Locked entries as `(url, name, hash)`, rejecting two different contents stored under one name.
fn locked_modules(lock: &ModuleLock) -> miette::Result<Vec<(String, String, String)>> {
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

fn read_lock(path: &Path) -> miette::Result<ModuleLock> {
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
    fs::write(lockfile, lock.to_json())
        .into_diagnostic()
        .wrap_err_with(|| format!("Failed to write {}", lockfile.display()))
}

impl Cli {
    /// `mq module add [--global] MODULE...`
    pub(super) fn module_add(specs: &[String], global: bool, domains: &[String]) -> miette::Result<()> {
        let target = Target::select(global)?;
        target.create_dir()?;
        let resolver = Self::module_resolver(&target, domains, false);
        for spec in specs {
            if !(http_import::is_github_url(spec) || http_import::is_remote_url(spec)) {
                bail!("`{spec}` is not a github.com/OWNER/REPO[@VERSION] or https:// module");
            }
            let name = module_name(spec)?;
            let hash = Self::fetch_module(&resolver, &target, spec, name)?;
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

    fn module_resolver(target: &Target, domains: &[String], frozen: bool) -> HttpModuleResolver<UreqFetcher> {
        let mut fetcher = UreqFetcher::default();
        fetcher.set_lockfile_path(target.lockfile.clone());
        fetcher.set_lockfile_frozen(frozen);
        let mut resolver = HttpModuleResolver::new(domains.to_vec(), fetcher);
        resolver.set_enabled(true);
        resolver
    }

    /// Fetches `spec` through the lock-checked resolver and writes it as `<name>.mq`, returning its hash.
    fn fetch_module(
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
}
