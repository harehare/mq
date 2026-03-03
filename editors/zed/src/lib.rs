use zed_extension_api::{self as zed, Result, settings::LspSettings};

struct MqExtension {
    cached_binary_path: Option<String>,
}

impl MqExtension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &zed::LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<String> {
        // Check if user has specified a custom binary path via settings
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
        let extension = match platform {
            zed::Os::Mac | zed::Os::Linux => "",
            zed::Os::Windows => "exe",
        };
        let asset_name = if extension.is_empty() {
            format!("mq-lsp-{arch}-{os}")
        } else {
            format!("mq-lsp-{arch}-{os}.{extension}")
        };

        let asset = release
            .assets
            .iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("no asset found matching {:?}", asset_name))?;

        let version_dir = format!("mq-lsp-{}", release.version);
        let binary_path = format!("{version_dir}/mq-lsp");

        if !std::fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );

            zed::download_file(&asset.download_url, &version_dir, zed::DownloadedFileType::Uncompressed)
                .map_err(|e| format!("failed to download file: {e}"))?;

            let entries = std::fs::read_dir(".").map_err(|e| format!("failed to list working directory {e}"))?;
            for entry in entries {
                let entry = entry.map_err(|e| format!("failed to load directory entry {e}"))?;
                if entry.file_name().to_str() != Some(&version_dir) {
                    std::fs::remove_dir_all(entry.path()).ok();
                }
            }

            zed::make_file_executable(&binary_path)?;
        }

        self.cached_binary_path = Some(binary_path.clone());
        Ok(binary_path)
    }

    /// Build type-checking CLI args from LSP initialization options.
    ///
    /// Reads `enableTypeCheck`, `strictArray`, and `tuple` from `initialization_options`
    /// and converts them to `mq-lsp` CLI flags.
    fn type_check_args_from_settings(worktree: &zed::Worktree) -> Vec<String> {
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

        if init_opts.get("tuple").and_then(|v| v.as_bool()).unwrap_or(false) {
            args.push("--tuple".to_string());
        }

        args
    }
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
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // Check if user specified custom binary arguments via settings
        let user_args = LspSettings::for_worktree("mq-lsp", worktree)
            .ok()
            .and_then(|s| s.binary)
            .and_then(|b| b.arguments);

        let args = if let Some(args) = user_args {
            args
        } else {
            Self::type_check_args_from_settings(worktree)
        };

        Ok(zed::Command {
            command: self.language_server_binary_path(language_server_id, worktree)?,
            args,
            env: Default::default(),
        })
    }
}

zed::register_extension!(MqExtension);
