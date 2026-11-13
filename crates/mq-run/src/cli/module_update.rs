//! `mq module update`: move tag-pinned modules to their latest tag.
use super::Cli;
use super::module::{Target, locked_modules, read_lock, remove_lock_entry};
use miette::{IntoDiagnostic, WrapErr, miette};
use mq_lang::UreqFetcher;
use serde_json::Value;

const RAW_PREFIX: &str = "https://raw.githubusercontent.com/";

/// The parts of a `raw.githubusercontent.com/OWNER/REPO/REF/FILE` URL.
#[derive(Debug, PartialEq, Eq)]
struct RawUrl<'a> {
    owner: &'a str,
    repo: &'a str,
    git_ref: &'a str,
    file: &'a str,
}

fn parse_raw_url(url: &str) -> Option<RawUrl<'_>> {
    let mut parts = url.strip_prefix(RAW_PREFIX)?.splitn(4, '/');
    Some(RawUrl {
        owner: parts.next()?,
        repo: parts.next()?,
        git_ref: parts.next()?,
        file: parts.next()?,
    })
}

type Version = (u64, u64, u64);

/// `v1`, `1.2` or `v1.2.3`; anything else (branches, pre-releases, hashes) is `None`.
fn parse_version(tag: &str) -> Option<Version> {
    let mut parts = tag.strip_prefix('v').unwrap_or(tag).split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = parts.next().map_or(Some(0), |p| p.parse().ok())?;
    parts.next().is_none().then_some((major, minor, patch))
}

/// The highest version among the tags in a GitHub `/tags` response.
fn latest_tag(tags_json: &str) -> miette::Result<Option<(Version, String)>> {
    let tags: Value = serde_json::from_str(tags_json)
        .into_diagnostic()
        .wrap_err("Invalid tag list")?;
    Ok(tags
        .as_array()
        .ok_or_else(|| miette!("Invalid tag list"))?
        .iter()
        .filter_map(|tag| tag["name"].as_str())
        .filter_map(|name| Some((parse_version(name)?, name.to_string())))
        .max_by_key(|(version, _)| *version))
}

impl Cli {
    /// `mq module update [NAME...] [--global] [--dry-run]`
    pub(super) fn module_update(
        names: &[String],
        global: bool,
        dry_run: bool,
        domains: &[String],
    ) -> miette::Result<()> {
        let target = Target::select(global)?;
        let modules = locked_modules(&read_lock(&target.lockfile)?)?;
        if let Some(unknown) = names.iter().find(|n| !modules.iter().any(|(_, name, _)| name == *n)) {
            return Err(miette!("`{unknown}` is not in {}", target.lockfile.display()));
        }

        let fetcher = UreqFetcher::default();
        let resolver = Self::module_resolver(&target, domains, false);
        for (url, name, _) in modules {
            if !names.is_empty() && !names.contains(&name) {
                continue;
            }
            let Some(raw) = parse_raw_url(&url) else {
                println!("{name}: not a GitHub module, skipped");
                continue;
            };
            let Some(current) = parse_version(raw.git_ref) else {
                println!("{name}: tracks `{}`, skipped", raw.git_ref);
                continue;
            };

            let tags_url = format!(
                "https://api.github.com/repos/{}/{}/tags?per_page=100",
                raw.owner, raw.repo
            );
            let tags = fetcher
                .fetch_uncached(&tags_url)
                .map_err(|e| miette!("Could not list the tags of {}/{}: {e}", raw.owner, raw.repo))?;
            let Some((latest, tag)) = latest_tag(&tags)? else {
                println!("{name}: no version tags found, skipped");
                continue;
            };
            if latest <= current {
                println!("{name}: {} is up to date", raw.git_ref);
                continue;
            }

            if dry_run {
                println!("{name}: {} -> {tag} (dry run)", raw.git_ref);
                continue;
            }
            let new_url = format!("{RAW_PREFIX}{}/{}/{tag}/{}", raw.owner, raw.repo, raw.file);
            target.create_dir()?;
            Self::fetch_module(&resolver, &target, &new_url, &name)?;
            remove_lock_entry(&target.lockfile, &url)?;
            println!("{name}: {} -> {tag}", raw.git_ref);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn test_parse_raw_url() {
        assert_eq!(
            parse_raw_url("https://raw.githubusercontent.com/harehare/kdl.mq/v1.0/kdl.mq"),
            Some(RawUrl {
                owner: "harehare",
                repo: "kdl.mq",
                git_ref: "v1.0",
                file: "kdl.mq"
            })
        );
        assert_eq!(parse_raw_url("https://example.com/a/b/c/d"), None);
        assert_eq!(parse_raw_url("https://raw.githubusercontent.com/a/b/c"), None);
    }

    #[rstest]
    #[case("v1.2.3", Some((1, 2, 3)))]
    #[case("1.2", Some((1, 2, 0)))]
    #[case("v3", Some((3, 0, 0)))]
    #[case("v1.0.0-beta", None)]
    #[case("v1.2.3.4", None)]
    #[case("HEAD", None)]
    #[case("main", None)]
    fn test_parse_version(#[case] tag: &str, #[case] expected: Option<Version>) {
        assert_eq!(parse_version(tag), expected);
    }

    #[test]
    fn test_latest_tag_compares_numerically_and_skips_non_versions() {
        let tags =
            r#"[{"name":"v0.9.0"},{"name":"v0.10.0"},{"name":"nightly"},{"name":"v0.10.0-rc1"},{"name":"v0.2.5"}]"#;

        assert_eq!(latest_tag(tags).unwrap(), Some(((0, 10, 0), "v0.10.0".to_string())));
        assert_eq!(latest_tag(r#"[{"name":"nightly"}]"#).unwrap(), None);
        assert!(latest_tag(r#"{"message":"rate limited"}"#).is_err());
    }
}
