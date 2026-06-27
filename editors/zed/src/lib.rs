use zed_extension_api::{
    self as zed, Result, SlashCommand, SlashCommandArgumentCompletion, SlashCommandOutput, SlashCommandOutputSection,
    Worktree, process::Command, settings::LspSettings,
};

struct MqExtension {
    cached_binary_path: Option<String>,
}

impl MqExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &Worktree,
    ) -> Result<String> {
        if let Ok(lsp_settings) = LspSettings::for_worktree("mq-lsp", worktree)
            && let Some(binary) = lsp_settings.binary
            && let Some(path) = binary.path
        {
            return Ok(path);
        }

        if let Some(path) = &self.cached_binary_path
            && std::fs::metadata(path).is_ok_and(|stat| stat.is_file())
        {
            return Ok(path.clone());
        }

        if let Some(path) = worktree.which("mq-lsp") {
            self.cached_binary_path = Some(path.clone());
            return Ok(path);
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        let release = zed::latest_github_release(
            "harehare/mq",
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (platform, arch) = zed::current_platform();
        let arch = match arch {
            zed::Architecture::Aarch64 => "aarch64",
            zed::Architecture::X8664 => "x86_64",
            zed::Architecture::X86 => return Err("unsupported architecture".into()),
        };
        let os = match platform {
            zed::Os::Mac => "apple-darwin",
            zed::Os::Linux => "unknown-linux-gnu",
            zed::Os::Windows => "pc-windows-msvc",
        };
        let asset_name = match platform {
            zed::Os::Windows => format!("mq-lsp-{arch}-{os}.exe"),
            _ => format!("mq-lsp-{arch}-{os}"),
        };

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("mq-lsp-{}", release.version);
        let binary_path = match platform {
            zed::Os::Windows => format!("{version_dir}/mq-lsp.exe"),
            _ => format!("{version_dir}/mq-lsp"),
        };

        if !std::fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            std::fs::create_dir_all(&version_dir).map_err(|e| format!("failed to create directory: {e}"))?;

            zed::download_file(&asset.download_url, &binary_path, zed::DownloadedFileType::Uncompressed)
                .map_err(|e| format!("failed to download file: {e}"))?;

            let entries = std::fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries.flatten() {
                if entry.file_name().to_str() != Some(&version_dir) {
                    let path = entry.path();
                    if path.is_dir() {
                        std::fs::remove_dir_all(path).ok();
                    } else {
                        std::fs::remove_file(path).ok();
                    }
                }
            }

            zed::make_file_executable(&binary_path)?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }

    fn type_check_args_from_settings(worktree: &Worktree) -> Vec<String> {
        let Ok(lsp_settings) = LspSettings::for_worktree("mq-lsp", worktree) else {
            return vec![];
        };
        let Some(init_opts) = lsp_settings.initialization_options else {
            return vec![];
        };

        let enable_type_check = init_opts
            .get("enableTypeCheck")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if !enable_type_check {
            return vec![];
        }

        let mut args = vec!["--enable-type-checking".to_string()];

        if init_opts.get("strictArray").and_then(|v| v.as_bool()).unwrap_or(false) {
            args.push("--strict-array".to_string());
        }

        args
    }

    fn lint_args_from_settings(worktree: &Worktree) -> Vec<String> {
        let Ok(lsp_settings) = LspSettings::for_worktree("mq-lsp", worktree) else {
            return vec![];
        };
        let Some(init_opts) = lsp_settings.initialization_options else {
            return vec![];
        };

        let enable_lint = init_opts.get("enableLint").and_then(|v| v.as_bool()).unwrap_or(false);

        if !enable_lint {
            return vec![];
        }

        let mut args = vec!["--enable-lint".to_string()];

        if let Some(rules) = init_opts.get("lintDisabledRules").and_then(|v| v.as_array()) {
            for rule_id in rules.iter().filter_map(|v| v.as_str()) {
                args.push("--disable-lint-rule".to_string());
                args.push(rule_id.to_string());
            }
        }

        args
    }
}

fn find_mq_binary(worktree: Option<&Worktree>) -> Result<String, String> {
    if let Some(wt) = worktree
        && let Some(path) = wt.which("mq")
    {
        return Ok(path);
    }
    Err("mq binary not found. Please install mq: https://mqlang.org".into())
}

/// Walk `dir` and collect paths of all `.md` / `.mdx` files, skipping hidden directories.
fn collect_md_files(dir: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };

    let mut files = vec![];
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden entries (e.g. .git, .cache)
        if name_str.starts_with('.') {
            continue;
        }

        if path.is_dir() {
            files.extend(collect_md_files(&path.to_string_lossy()));
        } else if let Some(ext) = path.extension()
            && (ext == "md" || ext == "mdx")
        {
            files.push(path.to_string_lossy().into_owned());
        }
    }
    files
}

/// Resolve the list of files to pass to `mq`.
/// - If `file_args` is non-empty, use those paths directly.
/// - Otherwise, gather all `.md` / `.mdx` files from the worktree root.
fn resolve_files(file_args: &[String], worktree: Option<&Worktree>) -> Result<Vec<String>, String> {
    if !file_args.is_empty() {
        return Ok(file_args.to_vec());
    }

    let root = worktree
        .map(|w| w.root_path())
        .ok_or("No worktree available. Please specify a file path.")?;

    let files = collect_md_files(&root);
    if files.is_empty() {
        Err("No Markdown files found in workspace.".into())
    } else {
        Ok(files)
    }
}

fn run_mq(
    query: &str,
    file_args: &[String],
    worktree: Option<&Worktree>,
    label: &str,
) -> Result<SlashCommandOutput, String> {
    let mq_binary = find_mq_binary(worktree)?;
    let files = resolve_files(file_args, worktree)?;

    let output = Command::new(&mq_binary)
        .arg(query)
        .args(files.iter().cloned())
        .output()
        .map_err(|e| format!("Failed to run mq: {e}"))?;

    if output.status.is_some_and(|s| s != 0) && !output.stderr.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("mq error: {stderr}"));
    }

    let text = if output.stdout.is_empty() {
        "No results found.".to_string()
    } else {
        String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 output: {e}"))?
    };

    let end = text.len() as u32;
    Ok(SlashCommandOutput {
        text,
        sections: vec![SlashCommandOutputSection {
            range: zed::Range { start: 0, end },
            label: label.to_string(),
        }],
    })
}

fn complete_md_files(prefix: &str) -> Vec<SlashCommandArgumentCompletion> {
    let (dir, file_prefix) = match prefix.rfind('/') {
        Some(pos) => (&prefix[..=pos], &prefix[pos + 1..]),
        None => (".", prefix),
    };

    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if name_str.starts_with('.') {
                return None;
            }

            if !name_str.starts_with(file_prefix) {
                return None;
            }

            let is_dir = path.is_dir();
            let is_md = path.extension().map(|e| e == "md" || e == "mdx").unwrap_or(false);

            if !is_dir && !is_md {
                return None;
            }

            let new_text = if dir == "." {
                if is_dir {
                    format!("{}/", name_str)
                } else {
                    name_str.to_string()
                }
            } else {
                if is_dir {
                    format!("{}{}/", dir, name_str)
                } else {
                    format!("{}{}", dir, name_str)
                }
            };

            Some(SlashCommandArgumentCompletion {
                label: new_text.clone(),
                new_text,
                run_command: !is_dir,
            })
        })
        .collect()
}

impl zed::Extension for MqExtension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let user_args = LspSettings::for_worktree("mq-lsp", worktree)
            .ok()
            .and_then(|s| s.binary)
            .and_then(|b| b.arguments);

        let args = if let Some(args) = user_args {
            args
        } else {
            let mut args = Self::type_check_args_from_settings(worktree);
            args.extend(Self::lint_args_from_settings(worktree));
            args
        };

        Ok(zed::Command {
            command: self.language_server_binary_path(language_server_id, worktree)?,
            args,
            env: Default::default(),
        })
    }

    fn complete_slash_command_argument(
        &self,
        command: SlashCommand,
        args: Vec<String>,
    ) -> Result<Vec<SlashCommandArgumentCompletion>, String> {
        match command.name.as_str() {
            "mq-outline" | "mq-code" | "mq-todo" | "mq-changelog" => {
                let prefix = args.last().map(String::as_str).unwrap_or("");
                Ok(complete_md_files(prefix))
            }
            "mq" => {
                // args[0] = query (no completion), args[1..] = files
                if args.len() >= 2 {
                    let prefix = args.last().map(String::as_str).unwrap_or("");
                    Ok(complete_md_files(prefix))
                } else {
                    Ok(vec![])
                }
            }
            _ => Ok(vec![]),
        }
    }

    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        match command.name.as_str() {
            "mq-outline" => run_mq(".h", &args, worktree, "mq: outline"),

            "mq-code" => run_mq(".code", &args, worktree, "mq: code blocks"),

            "mq-todo" => run_mq(".list | select(.checked == false)", &args, worktree, "mq: todos"),

            "mq-changelog" => run_mq(".h2 | first", &args, worktree, "mq: changelog"),

            "mq" => {
                if args.is_empty() {
                    return Err("Usage: /mq <query> [file ...]".into());
                }
                let query = &args[0];
                let file_args = &args[1..];
                run_mq(query, file_args, worktree, &format!("mq: {query}"))
            }

            _ => Err(format!("Unknown command: {}", command.name)),
        }
    }
}

zed::register_extension!(MqExtension);
