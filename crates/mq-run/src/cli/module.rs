//! `mq module`: copy HTTP modules into a local module directory.
use super::Cli;
use super::module_ui::{self as ui, Change, Pending};
use colored::Colorize;
use miette::{IntoDiagnostic, WrapErr, bail, miette};
use mq_lang::{HttpModuleResolver, LOCKFILE_NAME, ModuleLock, ModuleResolver, UreqFetcher, compute_hash, http_import};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Where vendored modules and their lock file live.
pub(super) struct Target {
    pub(super) dir: PathBuf,
    pub(super) lockfile: PathBuf,
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

    pub(super) fn select(global: bool) -> miette::Result<Self> {
        if global { Self::global() } else { Ok(Self::project()) }
    }

    pub(super) fn create_dir(&self) -> miette::Result<()> {
        fs::create_dir_all(&self.dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("Failed to create {}", self.dir.display()))
    }

    pub(super) fn module_path(&self, name: &str) -> PathBuf {
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

/// One locked module and whether its vendored file still matches the lock.
struct Checked {
    name: String,
    path: PathBuf,
    problem: Option<String>,
}

/// Compares vendored files with `lock`.
fn check_modules(target: &Target, lock: &ModuleLock) -> miette::Result<Vec<Checked>> {
    let mut checked = Vec::new();
    for (url, name, hash) in locked_modules(lock)? {
        let path = target.module_path(&name);
        let problem = match fs::read_to_string(&path) {
            Ok(content) if compute_hash(&content) == hash => None,
            Ok(_) => Some(format!("does not match the locked content of {url}")),
            Err(_) => Some(format!("is missing (locked as {url})")),
        };
        checked.push(Checked { name, path, problem });
    }
    Ok(checked)
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
        let started = Instant::now();
        let target = Target::select(global)?;
        for spec in specs {
            if !(http_import::is_github_url(spec) || http_import::is_remote_url(spec)) {
                bail!("`{spec}` is not a github.com/OWNER/REPO[@VERSION] or https:// module");
            }
            module_name(spec)?;
        }
        target.create_dir()?;
        let resolver = Self::module_resolver(&target, domains, false);

        ui::header(&format!(
            "Adding {} to {}",
            ui::count(specs.len()),
            target.dir.display()
        ));
        let width = specs
            .iter()
            .filter_map(|s| module_name(s).ok())
            .map(str::len)
            .max()
            .unwrap_or(0);
        let mut names = Vec::new();
        let mut counts = Counts::default();
        for (i, spec) in specs.iter().enumerate() {
            let name = module_name(spec)?;
            let fetched = Self::fetch_step(&resolver, &target, spec, name, i + 1, specs.len(), width)?;
            prune_replaced(&target.lockfile, name, &fetched.hash)?;
            counts.record(fetched.change);
            names.push(name);
        }

        ui::header(&format!(
            "Done in {}: {}. Pinned in {}",
            ui::format_duration(started.elapsed()),
            counts.summary(),
            target.lockfile.display()
        ));
        if let [name] = names.as_slice() {
            ui::hint(&format!("Use it with `import \"{name}\"`"));
        }
        Ok(())
    }

    /// `mq module install [--global]`
    pub(super) fn module_install(global: bool, domains: &[String]) -> miette::Result<()> {
        let started = Instant::now();
        let target = Target::select(global)?;
        let modules = locked_modules(&read_lock(&target.lockfile)?)?;
        if modules.is_empty() {
            bail!("{} has no modules to install", target.lockfile.display());
        }
        target.create_dir()?;
        // The lock file already pins every URL, so an install must never record new content.
        let resolver = Self::module_resolver(&target, domains, true);

        ui::header(&format!(
            "Installing {} from {} into {}",
            ui::count(modules.len()),
            target.lockfile.display(),
            target.dir.display()
        ));
        let width = modules.iter().map(|(_, n, _)| n.len()).max().unwrap_or(0);
        let mut counts = Counts::default();
        for (i, (url, name, _)) in modules.iter().enumerate() {
            let fetched = Self::fetch_step(&resolver, &target, url, name, i + 1, modules.len(), width)?;
            counts.record(fetched.change);
        }

        ui::header(&format!(
            "Done in {}: {}",
            ui::format_duration(started.elapsed()),
            counts.summary()
        ));
        Ok(())
    }

    /// `mq module check [--global]`
    pub(super) fn module_check(global: bool) -> miette::Result<()> {
        let target = Target::select(global)?;
        let checked = check_modules(&target, &read_lock(&target.lockfile)?)?;

        ui::header(&format!(
            "Checking {} against {}",
            ui::count(checked.len()),
            target.lockfile.display()
        ));
        let width = checked.iter().map(|c| c.name.len()).max().unwrap_or(0);
        for c in &checked {
            let name = format!("{:<width$}", c.name);
            match &c.problem {
                None => eprintln!(
                    "  {} {}  {}",
                    ui::ok_mark(),
                    name.bold(),
                    c.path.display().to_string().dimmed()
                ),
                Some(problem) => eprintln!("  {} {}  {} {problem}", ui::fail_mark(), name.bold(), c.path.display()),
            }
        }

        let problems = checked.iter().filter(|c| c.problem.is_some()).count();
        if problems == 0 {
            ui::header(&format!("All {} match the lock file", ui::count(checked.len())));
            return Ok(());
        }
        ui::hint("Run `mq module install` to restore them from the lock file");
        bail!("{} out of date", ui::count(problems));
    }

    pub(super) fn module_resolver(
        target: &Target,
        domains: &[String],
        frozen: bool,
    ) -> HttpModuleResolver<UreqFetcher> {
        let mut fetcher = UreqFetcher::default();
        fetcher.set_lockfile_path(target.lockfile.clone());
        fetcher.set_lockfile_frozen(frozen);
        let mut resolver = HttpModuleResolver::new(domains.to_vec(), fetcher);
        resolver.set_enabled(true);
        resolver
    }

    /// Fetches `spec` through the lock-checked resolver and writes it as `<name>.mq`.
    pub(super) fn fetch_module(
        resolver: &HttpModuleResolver<UreqFetcher>,
        target: &Target,
        spec: &str,
        name: &str,
    ) -> miette::Result<Fetched> {
        let content = resolver.resolve(spec).map_err(|e| miette!(e.to_string()))?;
        let path = target.module_path(name);
        let hash = compute_hash(&content);
        let change = match fs::read_to_string(&path) {
            Ok(old) if compute_hash(&old) == hash => Change::Unchanged,
            Ok(_) => Change::Updated,
            Err(_) => Change::Added,
        };
        if change != Change::Unchanged {
            fs::write(&path, &content)
                .into_diagnostic()
                .wrap_err_with(|| format!("Failed to write {}", path.display()))?;
        }
        Ok(Fetched {
            hash,
            path,
            bytes: content.len(),
            change,
        })
    }

    /// `fetch_module` with a `[i/n]` progress line and a one-line result.
    fn fetch_step(
        resolver: &HttpModuleResolver<UreqFetcher>,
        target: &Target,
        spec: &str,
        name: &str,
        index: usize,
        total: usize,
        width: usize,
    ) -> miette::Result<Fetched> {
        let pending = Pending::start(index, total, name);
        match Self::fetch_module(resolver, target, spec, name) {
            Ok(fetched) => {
                let change = match fetched.change {
                    Change::Added => fetched.change.label().green(),
                    Change::Updated => fetched.change.label().yellow(),
                    Change::Unchanged => fetched.change.label().dimmed(),
                };
                pending.finish(&format!(
                    "  {} {}  {change:<9}  {}  {}  {}",
                    ui::ok_mark(),
                    format!("{name:<width$}").bold(),
                    fetched.path.display(),
                    ui::format_size(fetched.bytes).dimmed(),
                    format!("sha256:{}", ui::short_hash(&fetched.hash)).dimmed()
                ));
                Ok(fetched)
            }
            Err(e) => {
                pending.finish(&format!(
                    "  {} {}  {spec}",
                    ui::fail_mark(),
                    format!("{name:<width$}").bold()
                ));
                Err(e)
            }
        }
    }
}

/// The result of writing one module to disk.
pub(super) struct Fetched {
    pub(super) hash: String,
    pub(super) path: PathBuf,
    pub(super) bytes: usize,
    pub(super) change: Change,
}

#[derive(Default)]
struct Counts {
    added: usize,
    updated: usize,
    unchanged: usize,
}

impl Counts {
    fn record(&mut self, change: Change) {
        match change {
            Change::Added => self.added += 1,
            Change::Updated => self.updated += 1,
            Change::Unchanged => self.unchanged += 1,
        }
    }

    /// `2 added, 1 unchanged`, leaving out zero counts.
    fn summary(&self) -> String {
        [
            (self.added, "added"),
            (self.updated, "updated"),
            (self.unchanged, "unchanged"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, label)| format!("{n} {label}"))
        .collect::<Vec<_>>()
        .join(", ")
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

        let checked = check_modules(&target, &lock).unwrap();

        assert_eq!(checked.len(), 3);
        assert_eq!(checked.iter().filter(|c| c.problem.is_some()).count(), 2);
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
