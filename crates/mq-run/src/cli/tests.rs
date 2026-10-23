#[rstest]
#[case("Runtime error: http: network access is disabled", Some("`--allow-net`"))]
#[case(
    "http: network access to https://api.example.invalid/x is not allowed (outside the allowed domains)",
    Some("`--allow-net=api.example.invalid`")
)]
#[case("read_file: filesystem reads are disabled", Some("`--allow-read`"))]
#[case(
    "read of /tmp/a.md is not allowed (outside the allowed paths)",
    Some("`--allow-read=/tmp/a.md`")
)]
#[case(
    "write of /tmp/a.md is not allowed (outside the allowed paths)",
    Some("`--allow-write=/tmp/a.md`")
)]
#[case("filesystem writes are disabled", Some("`--allow-write`"))]
#[case("environment variable access is disabled", Some("`--allow-env`"))]
#[case("access to environment variable `HOME` is not allowed", Some("`--allow-env=HOME`"))]
#[case("process execution is disabled", Some("`--allow-run`"))]
#[case(
    "execution of git is not allowed (outside the allowed commands)",
    Some("`--allow-run=git`")
)]
#[case("Unexpected token `x`", None)]
fn test_capability_hint(#[case] message: &str, #[case] expected: Option<&str>) {
    let hint = Cli::capability_hint(message);
    match expected {
        Some(flag) => {
            let hint = hint.expect("a hint");
            assert!(hint.contains(flag), "{hint} should contain {flag}");
            assert!(hint.contains("--allow-all"));
        }
        None => assert!(hint.is_none()),
    }
}

use rstest::rstest;
use scopeguard::defer;
use std::io::Write;
use std::{fs::File, path::PathBuf};

use super::*;

fn create_file(name: &str, content: &str) -> (PathBuf, PathBuf) {
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join(name);
    let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
    file.write_all(content.as_bytes())
        .expect("Failed to write to temp file");

    (temp_dir, temp_file_path)
}

#[test]
fn test_cli_null_input() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some("self".to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_cli_does_not_auto_collect_final_coroutine_output() {
    let (_, output_file) = create_file("test_cli_does_not_auto_collect_final_coroutine_output.md", "");
    let output_file_cleanup = output_file.clone();
    defer! {
        if output_file_cleanup.exists() {
            std::fs::remove_file(&output_file_cleanup).expect("Failed to delete temporary output file");
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("def source(): yield: 1 | yield: 2 | yield: 3; | source() | map(fn(x): x * 10;)".to_string()),
        files: None,
        ..Cli::default()
    };

    cli.run().unwrap();
    assert_eq!(std::fs::read_to_string(output_file).unwrap().trim(), "coroutine");
}

#[test]
fn test_cli_explicit_collect_still_materializes_coroutine_output() {
    let (_, output_file) = create_file("test_cli_explicit_collect_still_materializes_coroutine_output.md", "");
    let output_file_cleanup = output_file.clone();
    defer! {
        if output_file_cleanup.exists() {
            std::fs::remove_file(&output_file_cleanup).expect("Failed to delete temporary output file");
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some(
            "def source(): yield: 1 | yield: 2 | yield: 3; | source() | map(fn(x): x * 10;) | collect()".to_string(),
        ),
        files: None,
        ..Cli::default()
    };

    cli.run().unwrap();
    assert_eq!(std::fs::read_to_string(output_file).unwrap().trim(), "[10, 20, 30]");
}

#[test]
fn test_cli_limit_does_not_hang_on_infinite_generator() {
    // `--limit` must not wait on a leftover coroutine: it is never auto-collected.
    let (_, output_file) = create_file("test_cli_limit_does_not_hang_on_infinite_generator.md", "");
    let output_file_cleanup = output_file.clone();
    defer! {
        if output_file_cleanup.exists() {
            std::fs::remove_file(&output_file_cleanup).expect("Failed to delete temporary output file");
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            limit: Some(3),
            ..Default::default()
        },
        commands: None,
        query: Some("def nat(): var i = 0 | while (true): yield: i | i += 1;; | nat()".to_string()),
        files: None,
        ..Cli::default()
    };

    cli.run().unwrap();
    assert_eq!(std::fs::read_to_string(output_file).unwrap().trim(), "coroutine");
}

#[test]
fn test_cli_does_not_auto_collect_nested_coroutine_output() {
    let (_, output_file) = create_file("test_cli_does_not_auto_collect_nested_coroutine_output.md", "");
    let output_file_cleanup = output_file.clone();
    defer! {
        if output_file_cleanup.exists() {
            std::fs::remove_file(&output_file_cleanup).expect("Failed to delete temporary output file");
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("def g(): yield: 1 | yield: 2; | [g()]".to_string()),
        files: None,
        ..Cli::default()
    };

    cli.run().unwrap();
    assert_eq!(std::fs::read_to_string(output_file).unwrap().trim(), "[coroutine]");
}

#[test]
fn test_allow_read_flag_gates_read_file() {
    let (_, temp_file_path) = create_file("test_allow_read.md", "hello");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    // Backslashes would be parsed as escapes inside an mq string literal on Windows.
    let query = format!("read_file(\"{}\")", temp_file_path.to_string_lossy().replace('\\', "/"));

    let blocked_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.clone()),
        files: None,
        ..Cli::default()
    };
    assert!(
        blocked_cli.run().is_err(),
        "read_file should be blocked without --allow-read"
    );

    let allowed_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                allow_read: Some(vec![]),
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query),
        files: None,
        ..Cli::default()
    };
    assert!(allowed_cli.run().is_ok(), "read_file should succeed with --allow-read");
}

#[test]
fn test_allow_run_flag_gates_system() {
    let query = "system(\"echo\", [\"hello\"])";

    let blocked_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };
    assert!(
        blocked_cli.run().is_err(),
        "system should be blocked without --allow-run"
    );

    let allowed_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                allow_run: Some(vec![]),
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };
    assert!(allowed_cli.run().is_ok(), "system should succeed with --allow-run");

    let restricted_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                allow_run: Some(vec!["ls".to_string()]),
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };
    assert!(
        restricted_cli.run().is_err(),
        "system should be blocked when --allow-run only names other commands"
    );
}

#[test]
fn test_allow_env_flag_gates_var_interpolation() {
    // SAFETY: no other threads read/write this env var concurrently in this test.
    unsafe { std::env::set_var("MQ_ALLOW_ENV_TEST_VAR", "test_value") };
    defer! {
        unsafe { std::env::remove_var("MQ_ALLOW_ENV_TEST_VAR") };
    }

    let query = "$MQ_ALLOW_ENV_TEST_VAR";

    let blocked_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };
    assert!(blocked_cli.run().is_err(), "$VAR should be blocked without --allow-env");

    let allowed_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                allow_env: Some(vec![]),
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };
    assert!(allowed_cli.run().is_ok(), "$VAR should succeed with --allow-env");

    let restricted_cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                allow_env: Some(vec!["MQ_OTHER_VAR".to_string()]),
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };
    assert!(
        restricted_cli.run().is_err(),
        "$VAR should be blocked when --allow-env only names other variables"
    );
}

#[test]
fn test_allow_all_flag_grants_every_capability() {
    // SAFETY: no other threads read/write this env var concurrently in this test.
    unsafe { std::env::set_var("MQ_ALLOW_ALL_TEST_VAR", "test_value") };
    defer! {
        unsafe { std::env::remove_var("MQ_ALLOW_ALL_TEST_VAR") };
    }

    let base_cli = |query: &str, allow_all: bool| Cli {
        input: InputArgs {
            program: ProgramArgs {
                allow_all,
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(
        base_cli("$MQ_ALLOW_ALL_TEST_VAR", false).run().is_err(),
        "$VAR should be blocked without --allow-all"
    );
    assert!(
        base_cli("$MQ_ALLOW_ALL_TEST_VAR", true).run().is_ok(),
        "$VAR should succeed with --allow-all"
    );

    let system_query = "system(\"echo\", [\"hi\"])";
    assert!(
        base_cli(system_query, false).run().is_err(),
        "system should be blocked without --allow-all"
    );
    assert!(
        base_cli(system_query, true).run().is_ok(),
        "system should succeed with --allow-all"
    );
}

#[rstest]
#[case(&["mq", "--allow-all", "--allow-read=/tmp", "self"])]
#[case(&["mq", "--allow-all", "--allow-write=/tmp", "self"])]
#[case(&["mq", "--allow-all", "--allow-net=example.com", "self"])]
#[case(&["mq", "--allow-all", "--allow-run", "self"])]
#[case(&["mq", "--allow-all", "--allow-env", "self"])]
#[case(&["mq", "--allow-all", "--allow-http-import", "self"])]
fn test_allow_all_conflicts_with_individual_allow_flags(#[case] args: &[&str]) {
    assert!(
        Cli::try_parse_from(args).is_err(),
        "--allow-all should conflict with individual --allow-* flags"
    );
}

#[rstest]
#[case(&["mq", "--sandbox=strict", "--allow-read=/tmp", "self"])]
#[case(&["mq", "--sandbox=networked", "--allow-net=example.com", "self"])]
#[case(&["mq", "--sandbox=unsafe", "--allow-run", "self"])]
#[case(&["mq", "--sandbox=unsafe", "--allow-env", "self"])]
#[case(&["mq", "--sandbox=unsafe", "--allow-http-import", "self"])]
#[case(&["mq", "--sandbox=unsafe", "--allow-all", "self"])]
fn test_sandbox_conflicts_with_individual_allow_flags(#[case] args: &[&str]) {
    assert!(
        Cli::try_parse_from(args).is_err(),
        "--sandbox should conflict with --allow-* flags (including --allow-all)"
    );
}

#[rstest]
#[case(Some(SandboxProfile::Strict), None, "out.md", false)]
#[case(Some(SandboxProfile::ReadOnly), None, "out.md", false)]
#[case(Some(SandboxProfile::Networked), None, "out.md", false)]
#[case(Some(SandboxProfile::Strict), Some(vec![]), "out.md", true)]
#[case(Some(SandboxProfile::Unsafe), None, "out.md", true)]
#[case(None, None, "out.md", true)]
fn test_output_file_gated_by_allow_write_under_sandbox(
    #[case] sandbox: Option<SandboxProfile>,
    #[case] allow_write: Option<Vec<PathBuf>>,
    #[case] output: &str,
    #[case] expected_ok: bool,
) {
    let mut cli = Cli::default();
    cli.input.program.sandbox = sandbox;
    cli.input.program.allow_write = allow_write;
    cli.output.output_file = Some(PathBuf::from(output));
    assert_eq!(cli.check_output_permitted().is_ok(), expected_ok);
}

#[rstest]
#[case("allowed/out.md", true)]
#[case("other/out.md", false)]
fn test_output_file_restricted_to_allow_write_paths(#[case] output: &str, #[case] expected_ok: bool) {
    // Real directories: roots that don't exist canonicalize differently from their children on Windows.
    let base = std::env::temp_dir().join("mq_test_output_allow_write");
    let allowed = base.join("allowed");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(base.join("other")).unwrap();

    let mut cli = Cli::default();
    cli.input.program.sandbox = Some(SandboxProfile::ReadOnly);
    cli.input.program.allow_write = Some(vec![allowed]);
    cli.output.output_file = Some(base.join(output));
    assert_eq!(cli.check_output_permitted().is_ok(), expected_ok);
}

#[test]
fn test_sandbox_strict_denies_everything() {
    let io = ProgramArgs {
        sandbox: Some(SandboxProfile::Strict),
        ..Default::default()
    }
    .build_sandboxed_io();

    assert!(!io.is_read_allowed());
    assert!(!io.is_write_allowed());
    assert!(!io.is_net_allowed());
    assert!(!io.is_run_allowed());
    assert!(!io.is_env_allowed());
}

#[test]
fn test_sandbox_read_only_grants_read_only() {
    let io = ProgramArgs {
        sandbox: Some(SandboxProfile::ReadOnly),
        ..Default::default()
    }
    .build_sandboxed_io();

    assert!(io.is_read_allowed());
    assert!(!io.is_write_allowed());
    assert!(!io.is_net_allowed());
    assert!(!io.is_run_allowed());
    assert!(!io.is_env_allowed());
}

#[test]
fn test_sandbox_networked_grants_net_only() {
    let io = ProgramArgs {
        sandbox: Some(SandboxProfile::Networked),
        ..Default::default()
    }
    .build_sandboxed_io();

    assert!(!io.is_read_allowed());
    assert!(!io.is_write_allowed());
    assert!(io.is_net_allowed());
    assert!(!io.is_run_allowed());
    assert!(!io.is_env_allowed());
}

#[test]
fn test_sandbox_unsafe_grants_every_capability() {
    let io = ProgramArgs {
        sandbox: Some(SandboxProfile::Unsafe),
        ..Default::default()
    }
    .build_sandboxed_io();

    assert!(io.is_read_allowed());
    assert!(io.is_write_allowed());
    assert!(io.is_net_allowed());
    assert!(io.is_run_allowed());
    assert!(io.is_env_allowed());
}

#[cfg(feature = "http-import")]
#[test]
fn test_sandbox_http_import_enablement() {
    assert!(!ProgramArgs::default().sandbox_enables_http_import());
    for (sandbox, expected) in [
        (SandboxProfile::ReadOnly, false),
        (SandboxProfile::Networked, true),
        (SandboxProfile::Unsafe, true),
    ] {
        let args = ProgramArgs {
            sandbox: Some(sandbox),
            ..Default::default()
        };
        assert_eq!(args.sandbox_enables_http_import(), expected);
    }
}

#[cfg(feature = "http-import")]
#[rstest]
#[case(&["mq", "--allow-http-import", "self"], true, &[])]
#[case(&["mq", "--allow-http-import=a.com,github.com/u/r", "self"], true, &["a.com", "github.com/u/r"])]
#[case(&["mq", "--allow-http-import=a.com", "--allow-http-import=b.com", "self"], true, &["a.com", "b.com"])]
#[case(&["mq", "--allowed-domain", "a.com", "self"], false, &["a.com"])]
#[case(&["mq", "--allow-http-import=a.com", "--allowed-domain", "b.com", "self"], true, &["a.com", "b.com"])]
#[case(&["mq", "--allow-all", "--allowed-domain", "a.com", "self"], true, &["a.com"])]
#[case(&["mq", "self"], false, &[])]
fn test_http_import_flags(#[case] args: &[&str], #[case] enabled: bool, #[case] domains: &[&str]) {
    let cli = Cli::try_parse_from(args).unwrap();
    assert_eq!(cli.input.program.http_import_enabled(), enabled);
    assert_eq!(cli.input.program.http_import_domains(), domains);
}

#[cfg(feature = "http-import")]
#[test]
fn test_allow_http_import_conflicts_with_allow_all() {
    assert!(Cli::try_parse_from(["mq", "--allow-all", "--allow-http-import=a.com", "self"]).is_err());
}

#[cfg(feature = "http-import")]
#[test]
fn test_http_import_disabled_by_default() {
    // No network call is made: resolution fails on the enabled-check before any
    // request, so this stays fast and deterministic regardless of connectivity.
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(r#"import "github.com/harehare/lisp""#.to_string()),
        files: None,
        ..Cli::default()
    };

    let err = cli.run().unwrap_err();
    assert!(
        err.to_string().contains("HTTP module imports are disabled"),
        "error was: {err}"
    );
}

#[cfg(feature = "http-import")]
#[test]
fn test_allowed_domain_alone_does_not_enable_http_import() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                allowed_domains: Some(vec!["example.com".to_string()]),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(r#"import "https://example.com/mod.mq""#.to_string()),
        files: None,
        ..Cli::default()
    };

    let err = cli.run().unwrap_err();
    assert!(
        err.to_string().contains("HTTP module imports are disabled"),
        "error was: {err}"
    );
}

#[cfg(feature = "http-import")]
#[test]
fn test_no_lockfile_conflicts_with_frozen() {
    assert!(
        Cli::try_parse_from(["mq", "--no-lockfile", "--frozen", "self"]).is_err(),
        "--no-lockfile should conflict with --frozen"
    );
}

#[rstest]
#[case("0")]
#[case("-1")]
#[case("NaN")]
#[case("inf")]
#[case("1e100")]
#[case("1e19")]
fn test_timeout_rejects_invalid_values(#[case] secs: &str) {
    let error = Cli::try_parse_from(["mq", "--timeout", secs, "self"]).unwrap_err();
    assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
    assert!(error.to_string().contains("--timeout"), "{error}");
}

#[test]
fn test_timeout_parses_fractional_seconds() {
    let cli = Cli::try_parse_from(["mq", "--timeout", "0.5", "self"]).unwrap();
    assert_eq!(cli.timeout, Some(Duration::from_millis(500)));
}

#[test]
fn test_timeout_aborts_infinite_loop() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some("while(true): 1;".to_string()),
        files: None,
        timeout: Some(Duration::from_millis(1)),
        ..Cli::default()
    };

    assert!(cli.run().is_err());
}

#[test]
fn test_timeout_allows_normal_query() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some("1 + 1".to_string()),
        files: None,
        timeout: Some(Duration::from_secs(5)),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[rstest]
#[case::default(false)]
#[case::enabled(true)]
fn test_stack_trace_flag(#[case] enabled: bool) {
    let cli = if enabled {
        Cli::try_parse_from(["mq", "--stack-trace", "self"]).unwrap()
    } else {
        Cli::try_parse_from(["mq", "self"]).unwrap()
    };
    let error = cli
        .create_engine()
        .unwrap()
        .eval(
            "def inner(): 1 / 0; def outer(): inner(); outer()",
            std::iter::once(mq_lang::RuntimeValue::None),
        )
        .unwrap_err()
        .to_string();

    assert_eq!(error.contains("stack trace:"), enabled);
    if enabled {
        assert!(error.contains("at inner (1:"), "{error}");
    }
}

#[rstest]
#[case::default_is_human(&["mq", "self"], ErrorFormat::Human)]
#[case::explicit_human(&["mq", "--error-format", "human", "self"], ErrorFormat::Human)]
#[case::json(&["mq", "--error-format", "json", "self"], ErrorFormat::Json)]
fn test_error_format_flag(#[case] args: &[&str], #[case] expected: ErrorFormat) {
    let cli = Cli::try_parse_from(args).unwrap();
    assert_eq!(cli.error_format, expected);
}

#[rstest]
#[case::short(&["mq", "-c", "-F", "json", "self"])]
#[case::long(&["mq", "--compact", "-F", "json", "self"])]
fn test_compact_flag(#[case] args: &[&str]) {
    let cli = Cli::try_parse_from(args).unwrap();
    assert!(cli.output.compact);
    assert!(!cli.output.count);
}

#[test]
fn test_count_has_no_short_flag() {
    assert!(Cli::try_parse_from(["mq", "--count", "self"]).unwrap().output.count);
}

#[test]
fn test_arg_and_args_flags() {
    let cli = Cli::try_parse_from(["mq", "--arg", "name", "Alice", "self", "--args", "x", "y"]).unwrap();
    assert_eq!(cli.input.arg, Some(vec!["name".to_string(), "Alice".to_string()]));
    assert_eq!(cli.args, Some(vec!["x".to_string(), "y".to_string()]));
}

#[rstest]
#[case::consumed_by_args(&["mq", "-I", "null", "--args", "a", "b", "ARGS"], true)]
#[case::args_last(&["mq", "-I", "null", "ARGS", "--args", "a", "b"], false)]
fn test_args_before_query_is_rejected(#[case] argv: &[&str], #[case] rejected: bool) {
    let cli = Cli::try_parse_from(argv).unwrap();
    assert_eq!(cli.validate_args_position().is_err(), rejected);
}

#[test]
fn test_argv_flag_is_removed() {
    assert!(Cli::try_parse_from(["mq", "self", "--argv", "x"]).is_err());
}

#[test]
fn test_actions_are_subcommands_not_flags() {
    assert!(matches!(
        Cli::try_parse_from(["mq", "commands"]).unwrap().commands,
        Some(Commands::Subcommands { json: false })
    ));
    assert!(Cli::try_parse_from(["mq", "list"]).unwrap().commands.is_none());
    assert!(Cli::try_parse_from(["mq", "--list"]).is_err());
}

#[cfg(feature = "http-import")]
#[test]
fn test_clear_cache_flag() {
    let cli = Cli::try_parse_from(["mq", "--clear-cache", "self"]).unwrap();
    assert!(cli.input.program.clear_cache);
}

#[rstest]
#[case::completion(&["mq", "--compact", "completion", "bash"])]
#[case::help(&["mq", "--compact", "help", "map"])]
#[case::commands(&["mq", "--compact", "commands"])]
fn test_standalone_commands_ignore_output_flags(#[case] args: &[&str]) {
    assert!(Cli::try_parse_from(args).unwrap().run().is_ok());
}

#[rstest]
#[case::without_stack_trace(false)]
#[case::with_stack_trace(true)]
fn test_render_error_json(#[case] stack_trace: bool) {
    let args: &[&str] = if stack_trace {
        &["mq", "--stack-trace", "self"]
    } else {
        &["mq", "self"]
    };
    let cli = Cli::try_parse_from(args).unwrap();
    let err: mq_lang::Error = *cli
        .create_engine()
        .unwrap()
        .eval("1 / 0", std::iter::once(mq_lang::RuntimeValue::None))
        .unwrap_err();
    let json = Cli::render_error_json(&err.into()).unwrap();

    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let message = value["message"].as_str().unwrap();
    assert!(message.contains("Division by zero"), "{message}");
    assert_eq!(message.contains("stack trace:"), stack_trace);
}

#[test]
fn test_cli_raw_input() {
    let (_, temp_file_path) = create_file("test1.md", "# test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Text),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_cli_output_formats() {
    let (_, temp_file_path) = create_file("test2.md", "# test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    for format in [
        OutputFormat::Markdown,
        OutputFormat::Html,
        OutputFormat::Text,
        OutputFormat::Table,
        OutputFormat::Grep,
        OutputFormat::Gron,
        OutputFormat::Csv,
        OutputFormat::Xml,
        OutputFormat::Yaml,
        OutputFormat::Toon,
        OutputFormat::Shell,
    ] {
        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: Some(format.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path.clone()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }
}

#[test]
fn test_cli_list_styles() {
    let (_, temp_file_path) = create_file("test3.md", "# test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    for style in [ListStyle::Dash, ListStyle::Plus, ListStyle::Star] {
        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                list_style: Some(style.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path.clone()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
    }
}

#[test]
fn test_cli_color_output() {
    let (_, temp_file_path) = create_file("test_color.md", "# test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            color: ColorChoice::Always,
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path.clone()]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_cli_update_flag() {
    let (_, temp_file_path) = create_file("test4.md", "# test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            update: true,
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_cli_with_module_names() {
    let (temp_dir, temp_file_path) = create_file("math.mq", "def math(): 42;");
    let (_, temp_md_file_path) = create_file("test.md", "# test");
    let temp_md_file_path_clone = temp_md_file_path.clone();

    defer! {
        if temp_file_path.exists() {
            std::fs::remove_file(&temp_file_path).expect("Failed to delete temp file");
        }

        if temp_md_file_path_clone.exists() {
            std::fs::remove_file(&temp_md_file_path_clone).expect("Failed to delete temp file");
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                module_names: Some(vec!["math".to_string()]),
                module_directories: Some(vec![temp_dir.clone()]),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some("math".to_owned()),
        files: Some(vec![temp_md_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_find_external_commands() {
    // find_external_commands searches ~/.local/bin and PATH for mq-* files
    let commands = Cli::find_external_commands();
    // We can't assert specific commands, but we can check the function works
    assert!(commands.iter().all(|cmd| !cmd.is_empty()));
}

#[test]
fn test_get_external_commands_dir() {
    // This test checks if the function returns a valid path or None
    let dir = Cli::get_external_commands_dir();
    if let Some(path) = dir {
        assert!(path.ends_with(".local/bin") || path.ends_with(".local\\bin"));
    }
}

#[test]
#[cfg(unix)]
fn test_collect_mq_commands_from_dir() {
    let temp_dir = std::env::temp_dir().join("mq-collect-test");
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    // Create test files: mq-foo, mq-bar, a non-mq file, and a non-executable mq file
    fs::write(temp_dir.join("mq-foo"), "").expect("Failed to write file");
    fs::write(temp_dir.join("mq-bar"), "").expect("Failed to write file");
    fs::write(temp_dir.join("other-cmd"), "").expect("Failed to write file");
    fs::write(temp_dir.join("mq-noexec"), "").expect("Failed to write file");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Set executable bit on mq-foo and mq-bar, but not mq-noexec
        fs::set_permissions(temp_dir.join("mq-foo"), fs::Permissions::from_mode(0o755))
            .expect("Failed to set permissions");
        fs::set_permissions(temp_dir.join("mq-bar"), fs::Permissions::from_mode(0o755))
            .expect("Failed to set permissions");
    }

    let mut seen = std::collections::HashSet::new();
    Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);

    assert_eq!(seen.len(), 2);
    assert!(seen.contains("foo"));
    assert!(seen.contains("bar"));
    assert!(!seen.contains("other-cmd"));
    assert!(!seen.contains("noexec"));
}

#[test]
#[cfg(unix)]
fn test_collect_mq_commands_from_dir_deduplicates() {
    let dir1 = std::env::temp_dir().join("mq-dedup-test-1");
    let dir2 = std::env::temp_dir().join("mq-dedup-test-2");
    fs::create_dir_all(&dir1).expect("Failed to create test directory");
    fs::create_dir_all(&dir2).expect("Failed to create test directory");

    defer! {
        if dir1.exists() {
            std::fs::remove_dir_all(&dir1).ok();
        }
        if dir2.exists() {
            std::fs::remove_dir_all(&dir2).ok();
        }
    }

    // Same command in both directories
    fs::write(dir1.join("mq-dup"), "").expect("Failed to write file");
    fs::write(dir2.join("mq-dup"), "").expect("Failed to write file");
    fs::write(dir2.join("mq-unique"), "").expect("Failed to write file");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(dir1.join("mq-dup"), fs::Permissions::from_mode(0o755)).expect("Failed to set permissions");
        fs::set_permissions(dir2.join("mq-dup"), fs::Permissions::from_mode(0o755)).expect("Failed to set permissions");
        fs::set_permissions(dir2.join("mq-unique"), fs::Permissions::from_mode(0o755))
            .expect("Failed to set permissions");
    }

    let mut seen = std::collections::HashSet::new();
    Cli::collect_mq_commands_from_dir(&dir1, &mut seen);
    Cli::collect_mq_commands_from_dir(&dir2, &mut seen);

    assert_eq!(seen.len(), 2);
    assert!(seen.contains("dup"));
    assert!(seen.contains("unique"));
}

#[test]
fn test_collect_mq_commands_from_nonexistent_dir() {
    let nonexistent = std::env::temp_dir().join("mq-nonexistent-dir");
    let mut seen = std::collections::HashSet::new();
    // Should not panic on nonexistent directory
    Cli::collect_mq_commands_from_dir(&nonexistent, &mut seen);
    assert!(seen.is_empty());
}

#[rstest]
#[case("foo", "foo")]
#[case("foo.exe", "foo.exe")]
#[case("foo.cmd", "foo.cmd")]
#[case("foo.bat", "foo.bat")]
#[case("foo.sh", "foo.sh")]
#[cfg(not(windows))]
fn test_strip_executable_extension_unix(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(Cli::strip_executable_extension(input), expected);
}

#[rstest]
#[case("foo.exe", "foo")]
#[case("foo.cmd", "foo")]
#[case("foo.bat", "foo")]
#[case("foo.com", "foo")]
#[case("foo", "foo")]
#[case("foo.sh", "foo.sh")]
#[case("foo.txt", "foo.txt")]
#[cfg(windows)]
fn test_strip_executable_extension_windows(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(Cli::strip_executable_extension(input), expected);
}

#[test]
fn test_run_external_subcommand_ignores_non_external() {
    let args = |v: &[&str]| v.iter().map(OsString::from).collect::<Vec<_>>();
    // Built-in subcommands, queries, options and a missing name are never external.
    assert!(Cli::run_external_subcommand(&args(&["mq"])).is_none());
    assert!(Cli::run_external_subcommand(&args(&["mq", "compile", "--help"])).is_none());
    assert!(Cli::run_external_subcommand(&args(&["mq", ".h1 | upcase", "a.md"])).is_none());
    assert!(Cli::run_external_subcommand(&args(&["mq", "--help"])).is_none());
    assert!(Cli::run_external_subcommand(&args(&["mq", "no-such-external-cmd", "--flag"])).is_none());
}

#[test]
fn test_external_command_execution() {
    // Create a temporary directory for testing
    let temp_dir = std::env::temp_dir().join("mq-run-test");
    let bin_dir = temp_dir.join(".mq").join("bin");
    fs::create_dir_all(&bin_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    // Create a test external command
    let test_cmd_path = bin_dir.join("mq-testcmd");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::write(&test_cmd_path, "#!/bin/sh\necho 'test output'").expect("Failed to write test command");
        let mut perms = fs::metadata(&test_cmd_path)
            .expect("Failed to get metadata")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&test_cmd_path, perms).expect("Failed to set permissions");
    }
    #[cfg(not(unix))]
    {
        fs::write(&test_cmd_path, "@echo off\necho test output").expect("Failed to write test command");
    }

    // Note: We can't easily test execute_external_command without modifying HOME
    // This test just verifies the command file was created correctly
    assert!(test_cmd_path.exists());
}

#[test]
fn test_input_format_mdx() {
    let (_, temp_file_path) = create_file("test_mdx.mdx", "# MDX test");
    let (_, output_file) = create_file("test_mdx_output.md", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Mdx),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(output_content.contains("# MDX test"), "Output should contain heading");
}

#[test]
fn test_input_format_html() {
    let (_, temp_file_path) = create_file("test_html.html", "<h1>HTML test</h1>");
    let (_, output_file) = create_file("test_html_output.md", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Html),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(
        output_content.contains("# HTML test"),
        "Output should contain converted heading"
    );
}

#[test]
fn test_output_format_json_markdown_input() {
    let (_, temp_file_path) = create_file("test_json_md_input.md", "# Test");
    let (_, output_file) = create_file("test_json_md_output.json", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
    assert!(parsed.is_array(), "Markdown JSON output should be an array");
    let nodes = parsed.as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["type"], "Heading", "Markdown heading should have type=Heading");
    assert!(
        nodes[0].get("position").is_some(),
        "position should be present by default"
    );
}

#[test]
fn test_output_format_json_markdown_input_no_position() {
    let (_, temp_file_path) = create_file("test_json_md_input_no_position.md", "# Test");
    let (_, output_file) = create_file("test_json_md_output_no_position.json", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(output_file.clone()),
            no_position: true,
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
    let nodes = parsed.as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["type"], "Heading");
    assert!(
        nodes[0].get("position").is_none(),
        "--no-position should omit position from the top-level node"
    );
    let children = nodes[0]["values"].as_array().expect("heading should have values");
    assert!(
        children.iter().all(|c| c.get("position").is_none()),
        "--no-position should omit position from child nodes too"
    );
}

#[test]
fn test_output_format_json_with_json_object_input() {
    let (_, temp_file_path) = create_file("test_json_obj_input.json", r#"{"id": 1, "name": "Alice"}"#);
    let (_, output_file) = create_file("test_json_obj_output.json", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
    assert!(parsed.is_object(), "JSON object input should output a JSON object");
    assert_eq!(parsed["id"], 1.0, "id field should be preserved");
    assert_eq!(parsed["name"], "Alice", "name field should be preserved");
    assert!(
        parsed.get("type").is_none(),
        "Output should not contain Markdown AST 'type' field"
    );
}

#[test]
fn test_output_format_json_with_json_array_input() {
    let (_, temp_file_path) = create_file(
        "test_json_arr_input.json",
        r#"[{"id": 1, "name": "Alice"}, {"id": 2, "name": "Bob"}]"#,
    );
    let (_, output_file) = create_file("test_json_arr_output.json", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
    assert!(parsed.is_array(), "JSON array input should output a JSON array");
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 2, "Array should have 2 elements");
    assert_eq!(arr[0]["id"], 1.0);
    assert_eq!(arr[0]["name"], "Alice");
    assert_eq!(arr[1]["id"], 2.0);
    assert_eq!(arr[1]["name"], "Bob");
    assert!(
        arr[0].get("type").is_none(),
        "Output should not contain Markdown AST fields"
    );
}

#[test]
fn test_output_format_gron() {
    let (_, temp_file_path) = create_file("test_gron_output.md", "# Title\n");
    let (_, output_file) = create_file("test_gron_output.gron", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Gron),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(output_content.contains("json[0].type = \"Heading\";\n"));
    assert!(output_content.contains("json[0].depth = 1;\n"));
}

#[test]
fn test_input_format_gron_reconstructs_data() {
    let (_, temp_file_path) = create_file(
        "test_gron_input.gron",
        "json = {};\njson.a = {};\njson.a.b = \"deep\";\njson.arr = [];\njson.arr[0] = 1;\njson.arr[1] = 2;\n",
    );
    let (_, output_file) = create_file("test_gron_input_output.json", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
    assert_eq!(parsed["a"]["b"], "deep");
    assert_eq!(parsed["arr"][0], 1.0);
    assert_eq!(parsed["arr"][1], 2.0);
}

#[test]
fn test_output_format_gron_then_input_format_gron_round_trip() {
    let (_, md_file) = create_file("test_gron_roundtrip.md", "# Title\n\nSome *text* here.\n");
    let (_, gron_file) = create_file("test_gron_roundtrip.gron", "");
    let (_, json_file) = create_file("test_gron_roundtrip.json", "");
    let md_file_clone = md_file.clone();
    let gron_file_clone = gron_file.clone();
    let json_file_clone = json_file.clone();

    defer! {
        if md_file_clone.exists() {
            std::fs::remove_file(&md_file_clone).ok();
        }
        if gron_file_clone.exists() {
            std::fs::remove_file(&gron_file_clone).ok();
        }
        if json_file_clone.exists() {
            std::fs::remove_file(&json_file_clone).ok();
        }
    }

    let to_gron = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Gron),
            output_file: Some(gron_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![md_file.clone()]),
        ..Cli::default()
    };
    assert!(to_gron.run().is_ok());

    let from_gron = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(json_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![gron_file]),
        ..Cli::default()
    };
    assert!(from_gron.run().is_ok());

    let direct_json = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(json_file.with_extension("direct.json")),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![md_file]),
        ..Cli::default()
    };
    assert!(direct_json.run().is_ok());

    let roundtrip_content = fs::read_to_string(&json_file).expect("Failed to read roundtrip output");
    let direct_content =
        fs::read_to_string(json_file.with_extension("direct.json")).expect("Failed to read direct output");
    fs::remove_file(json_file.with_extension("direct.json")).ok();

    let roundtrip_value: serde_json::Value =
        serde_json::from_str(&roundtrip_content).expect("Roundtrip output should be valid JSON");
    let direct_value: serde_json::Value =
        serde_json::from_str(&direct_content).expect("Direct output should be valid JSON");
    assert_eq!(roundtrip_value, direct_value);
}

#[test]
fn test_output_format_raw() {
    let (_, output_file) = create_file("test_raw_output.bin", "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some(r#"to_bytes("hello")"#.to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_bytes = fs::read(&output_file).expect("Failed to read output");
    assert_eq!(output_bytes, b"hello");
}

#[rstest]
#[case::from_string("raw_from_string", r#"to_bytes("hello")"#, b"hello" as &[u8])]
#[case::from_number_array("raw_from_array", "to_bytes([104, 101, 108, 108, 111])", b"hello")]
#[case::binary_data("raw_binary", "to_bytes([0, 255, 128, 1])", &[0u8, 255, 128, 1])]
#[case::non_bytes_string("raw_string_value", r#""hello""#, b"hello\n")]
#[case::utf8("raw_utf8", r#"to_bytes("あ")"#, &[0xe3u8, 0x81, 0x82])]
#[case::empty("raw_empty", "to_bytes([])", b"")]
fn test_output_format_raw_bytes(#[case] suffix: &str, #[case] query: &str, #[case] expected: &[u8]) {
    let (_, output_file) = create_file(&format!("test_{}.bin", suffix), "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some(query.to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_bytes = fs::read(&output_file).expect("Failed to read output");
    assert_eq!(output_bytes, expected);
}

#[test]
fn test_output_format_none() {
    let (_, temp_file_path) = create_file("test_none.md", "# Test");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::None),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_output_format_table_single_column() {
    let (_, temp_file_path) = create_file("test_table.md", "# Test\n\nContent");
    let (_, output_file) = create_file("test_table_output.md", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Table),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(output_content.contains("value"), "Table should have value header");
    assert!(output_content.contains("Test"), "Table should contain node text");
}

#[test]
fn test_output_format_table_dict() {
    let (_, output_file) = create_file("test_table_dict_output.md", "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Table),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some(r#"{name: "Alice", age: "30"}"#.to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(output_content.contains("name"), "Table should contain name column");
    assert!(output_content.contains("age"), "Table should contain age column");
    assert!(output_content.contains("Alice"), "Table should contain Alice");
    assert!(output_content.contains("30"), "Table should contain 30");
}

#[test]
fn test_output_format_table_nested_dict() {
    let (_, output_file) = create_file("test_table_nested_dict_output.md", "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Table),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some(r#"{name: "Alice", addr: {city: "Tokyo", zip: "100"}}"#.to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(output_content.contains("addr"), "Table should contain addr column");
    assert!(output_content.contains("name"), "Table should contain name column");
    assert!(output_content.contains("Alice"), "Table should contain Alice");
    assert!(output_content.contains("city"), "Nested table should contain city key");
    assert!(output_content.contains("Tokyo"), "Nested table should contain Tokyo");
    assert!(output_content.contains("zip"), "Nested table should contain zip key");
    assert!(output_content.contains("100"), "Nested table should contain 100");
    assert!(!output_content.contains("addr.city"), "Dot notation must not appear");
}

#[test]
fn test_output_format_table_array_value() {
    let (_, output_file) = create_file("test_table_array_value_output.md", "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Table),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some(r#"{name: "Alice", tags: ["a", "b"]}"#.to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(output_content.contains("tags"), "Table should contain tags column");
    assert!(output_content.contains('a'), "Nested table should contain a");
    assert!(output_content.contains('b'), "Nested table should contain b");
    assert!(output_content.contains("Alice"), "Table should contain Alice");
    assert!(!output_content.contains(r#"["a""#), "Raw array repr must not appear");
}

#[test]
fn test_output_format_table_array_input() {
    let (_, output_file) = create_file("test_table_array_input_output.md", "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Table),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some(r#"[{a: "1"}, {a: "2"}]"#.to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(output_content.contains('a'), "Table should have column 'a'");
    assert!(output_content.contains('1'), "Row 1 value should appear");
    assert!(output_content.contains('2'), "Row 2 value should appear");
    assert!(
        !output_content.contains("value"),
        "Should not fall back to 'value' column"
    );
}

#[test]
fn test_link_title_styles() {
    let (_, temp_file_path) = create_file("test_link_title.md", "[link](url \"title\")");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
    }

    for (style, expected_char) in [
        (LinkTitleStyle::Double, '"'),
        (LinkTitleStyle::Single, '\''),
        (LinkTitleStyle::Paren, '('),
    ] {
        let (_, output_file) = create_file(&format!("test_link_title_{:?}.md", style), "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                link_title_style: style.clone(),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path.clone()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        if style == LinkTitleStyle::Paren {
            assert!(
                output_content.contains("(title)"),
                "Paren style should wrap title with parens"
            );
        } else {
            assert!(
                output_content.contains(expected_char),
                "Link title should use {:?} style",
                style
            );
        }
    }
}

#[test]
fn test_link_url_styles() {
    let (_, temp_file_path) = create_file("test_link_url.md", "[link](https://example.com)");
    let temp_file_path_clone = temp_file_path.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
    }

    for style in [LinkUrlStyle::None, LinkUrlStyle::Angle] {
        let (_, output_file) = create_file(&format!("test_link_url_{:?}.md", style), "");
        let output_file_clone = output_file.clone();

        defer! {
            if output_file_clone.exists() {
                std::fs::remove_file(&output_file_clone).ok();
            }
        }

        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                link_url_style: style.clone(),
                output_file: Some(output_file.clone()),
                ..Default::default()
            },
            commands: None,
            query: Some("self".to_string()),
            files: Some(vec![temp_file_path.clone()]),
            ..Cli::default()
        };

        assert!(cli.run().is_ok());
        let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
        if style == LinkUrlStyle::Angle {
            assert!(
                output_content.contains("<https://example.com>"),
                "Angle style should wrap URL with angle brackets"
            );
        } else {
            assert!(
                output_content.contains("(https://example.com)"),
                "None style should not wrap URL"
            );
        }
    }
}

#[test]
fn test_aggregate_flag() {
    let (_, temp_file1) = create_file("test_agg1.md", "# Test 1");
    let (_, temp_file2) = create_file("test_agg2.md", "# Test 2");
    let (_, output_file) = create_file("test_agg_output.md", "");
    let temp_file1_clone = temp_file1.clone();
    let temp_file2_clone = temp_file2.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file1_clone.exists() {
            std::fs::remove_file(&temp_file1_clone).ok();
        }
        if temp_file2_clone.exists() {
            std::fs::remove_file(&temp_file2_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                aggregate: true,
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            output_format: Some(OutputFormat::Text),
            ..Default::default()
        },
        commands: None,
        query: Some("len()".to_string()),
        files: Some(vec![temp_file1, temp_file2]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(!output_content.is_empty(), "Aggregated output should not be empty");
}

#[test]
fn test_eval_all_combines_nodes_across_files() {
    let (_, temp_file1) = create_file("test_eval_all1.md", "# File One\n\n## Sub A");
    let (_, temp_file2) = create_file("test_eval_all2.md", "# File Two\n\n## Sub B");
    let (_, output_file) = create_file("test_eval_all_output.md", "");
    let temp_file1_clone = temp_file1.clone();
    let temp_file2_clone = temp_file2.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file1_clone.exists() {
            std::fs::remove_file(&temp_file1_clone).ok();
        }
        if temp_file2_clone.exists() {
            std::fs::remove_file(&temp_file2_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    // -A alone aggregates per file (2 separate counts); --eval-all -A must combine
    // both files into a single collection before "nodes" runs, giving one total.
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                aggregate: true,
                ..Default::default()
            },
            eval_all: true,
            ..Default::default()
        },
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            output_format: Some(OutputFormat::Text),
            ..Default::default()
        },
        commands: None,
        query: Some(".h | len".to_string()),
        files: Some(vec![temp_file1, temp_file2]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(
        output_content.trim(),
        "4",
        "should count headings across both files as one"
    );
}

#[test]
fn test_eval_all_rejects_mixed_input_formats() {
    let (_, temp_md) = create_file("test_eval_all_mixed.md", "# Test");
    let (_, temp_json) = create_file("test_eval_all_mixed.json", r#"{"a":1}"#);
    let temp_md_clone = temp_md.clone();
    let temp_json_clone = temp_json.clone();

    defer! {
        if temp_md_clone.exists() {
            std::fs::remove_file(&temp_md_clone).ok();
        }
        if temp_json_clone.exists() {
            std::fs::remove_file(&temp_json_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            eval_all: true,
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_md, temp_json]),
        ..Cli::default()
    };

    let err = cli.run().unwrap_err();
    assert!(err.to_string().contains("--eval-all requires"));
}

#[test]
fn test_from_file_flag() {
    let (_, query_file) = create_file("test_query.mq", "self");
    let (_, input_file) = create_file("test_from_file.md", "# Test");
    let query_file_clone = query_file.clone();
    let input_file_clone = input_file.clone();

    defer! {
        if query_file_clone.exists() {
            std::fs::remove_file(&query_file_clone).ok();
        }
        if input_file_clone.exists() {
            std::fs::remove_file(&input_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            from_file: true,
            ..Default::default()
        },
        output: OutputArgs::default(),
        commands: None,
        query: Some(query_file.to_string_lossy().to_string()),
        files: Some(vec![input_file]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_separator_flag() {
    let (_, temp_file1) = create_file("test_sep1.md", "# Test 1");
    let (_, temp_file2) = create_file("test_sep2.md", "# Test 2");
    let (_, output_file) = create_file("test_sep_output.md", "");
    let temp_file1_clone = temp_file1.clone();
    let temp_file2_clone = temp_file2.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file1_clone.exists() {
            std::fs::remove_file(&temp_file1_clone).ok();
        }
        if temp_file2_clone.exists() {
            std::fs::remove_file(&temp_file2_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            separator: Some("\"---\"".to_string()),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file1, temp_file2]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(!output_content.is_empty(), "Output should not be empty");
    assert!(output_content.contains("# Test"), "File content should be present");
}

#[test]
fn test_output_file_flag() {
    let (_, temp_input) = create_file("test_input_out.md", "# Test Output");
    let temp_output = std::env::temp_dir().join("test_output_file.md");
    let temp_input_clone = temp_input.clone();
    let temp_output_clone = temp_output.clone();

    defer! {
        if temp_input_clone.exists() {
            std::fs::remove_file(&temp_input_clone).ok();
        }
        if temp_output_clone.exists() {
            std::fs::remove_file(&temp_output_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_file: Some(temp_output.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_input]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    assert!(temp_output.exists(), "Output file should exist");
    let output_content = fs::read_to_string(&temp_output).expect("Failed to read output");
    assert!(
        output_content.contains("# Test Output"),
        "Output content should match input"
    );
}

#[test]
fn test_unbuffered_output() {
    let (_, temp_file) = create_file("test_unbuf.md", "# Test");
    let temp_file_clone = temp_file.clone();

    defer! {
        if temp_file_clone.exists() {
            std::fs::remove_file(&temp_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            unbuffered: true,
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_update_with_non_markdown_input() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Html),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            update: true,
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: None,
        ..Cli::default()
    };

    assert!(cli.run().is_err());
}

#[test]
fn test_run_commands() {
    let cli = Cli {
        commands: Some(Commands::Subcommands { json: false }),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[test]
fn test_parallel_threshold() {
    let files: Vec<PathBuf> = (0..15)
        .map(|i| {
            let (_, path) = create_file(&format!("test_parallel_{}.md", i), "# Test");
            path
        })
        .collect();

    let files_clone = files.clone();
    defer! {
        for file in &files_clone {
            if file.exists() {
                std::fs::remove_file(file).ok();
            }
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs::default(),
        commands: None,
        query: Some("self".to_string()),
        files: Some(files),
        parallel_threshold: 10,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
}

#[rstest]
#[case("mq-exec-owner", 0o700, true)]
#[case("mq-exec-group", 0o010, true)]
#[case("mq-exec-other", 0o001, true)]
#[case("mq-exec-all", 0o755, true)]
#[case("mq-noexec-rw", 0o644, false)]
#[case("mq-noexec-ro", 0o444, false)]
#[cfg(unix)]
fn test_is_executable_file_unix(#[case] filename: &str, #[case] mode: u32, #[case] expected: bool) {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = std::env::temp_dir().join(format!("mq-exec-test-{filename}"));
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    let file_path = temp_dir.join(filename);
    fs::write(&file_path, "#!/bin/sh\necho test").expect("Failed to write file");
    fs::set_permissions(&file_path, fs::Permissions::from_mode(mode)).expect("Failed to set permissions");

    let entry = fs::read_dir(&temp_dir)
        .expect("Failed to read dir")
        .find(|e| e.as_ref().unwrap().file_name().to_str() == Some(filename))
        .unwrap()
        .unwrap();

    assert_eq!(
        Cli::is_executable_file(&entry),
        expected,
        "File with mode {mode:#o} should return {expected}"
    );
}

#[test]
#[cfg(unix)]
fn test_is_executable_file_unix_directory() {
    use std::os::unix::fs::PermissionsExt;

    let temp_dir = std::env::temp_dir().join("mq-dir-test-unix");
    let sub_dir = temp_dir.join("mq-subdir");
    fs::create_dir_all(&sub_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    fs::set_permissions(&sub_dir, fs::Permissions::from_mode(0o755)).expect("Failed to set permissions");

    let entry = fs::read_dir(&temp_dir)
        .expect("Failed to read dir")
        .find(|e| e.as_ref().unwrap().file_name() == "mq-subdir")
        .unwrap()
        .unwrap();

    assert!(!Cli::is_executable_file(&entry), "Directory should return false");
}

#[rstest]
#[case("mq-test.exe", true)]
#[case("mq-test.cmd", true)]
#[case("mq-test.bat", true)]
#[case("mq-test.com", true)]
#[case("mq-test.EXE", true)]
#[case("mq-test.Bat", true)]
#[case("mq-test.txt", false)]
#[case("mq-test.sh", false)]
#[case("mq-test", false)]
#[cfg(windows)]
fn test_is_executable_file_windows(#[case] filename: &str, #[case] expected: bool) {
    let temp_dir = std::env::temp_dir().join(format!("mq-exec-test-win-{}", filename.replace('.', "-")));
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    let file_path = temp_dir.join(filename);
    fs::write(&file_path, "test").expect("Failed to write file");

    let entry = fs::read_dir(&temp_dir)
        .expect("Failed to read dir")
        .find(|e| e.as_ref().unwrap().file_name().to_str() == Some(filename))
        .unwrap()
        .unwrap();

    assert_eq!(
        Cli::is_executable_file(&entry),
        expected,
        "File '{filename}' should return {expected}"
    );
}

#[test]
#[cfg(windows)]
fn test_is_executable_file_windows_directory() {
    let temp_dir = std::env::temp_dir().join("mq-dir-test-windows");
    let sub_dir = temp_dir.join("mq-subdir");
    fs::create_dir_all(&sub_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    let entry = fs::read_dir(&temp_dir)
        .expect("Failed to read dir")
        .find(|e| e.as_ref().unwrap().file_name() == "mq-subdir")
        .unwrap()
        .unwrap();

    assert!(!Cli::is_executable_file(&entry), "Directory should return false");
}

#[test]
#[cfg(not(any(unix, windows)))]
fn test_is_executable_file_other_os() {
    let temp_dir = std::env::temp_dir().join("mq-other-test");
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    let file = temp_dir.join("mq-test");
    fs::write(&file, "test").expect("Failed to write file");

    let entry = fs::read_dir(&temp_dir)
        .expect("Failed to read dir")
        .find(|e| e.as_ref().unwrap().file_name() == "mq-test")
        .unwrap()
        .unwrap();

    assert!(
        Cli::is_executable_file(&entry),
        "Regular file should return true on other OS"
    );
}

#[test]
#[cfg(not(any(unix, windows)))]
fn test_is_executable_file_other_os_directory() {
    let temp_dir = std::env::temp_dir().join("mq-dir-other-test");
    let sub_dir = temp_dir.join("mq-subdir");
    fs::create_dir_all(&sub_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    let entry = fs::read_dir(&temp_dir)
        .expect("Failed to read dir")
        .find(|e| e.as_ref().unwrap().file_name() == "mq-subdir")
        .unwrap()
        .unwrap();

    assert!(
        !Cli::is_executable_file(&entry),
        "Directory should return false on other OS"
    );
}

/// Test that Windows deduplicates commands with different executable extensions.
/// e.g., mq-foo.bat and mq-foo.exe in the same directory should produce only "foo".
#[test]
#[cfg(windows)]
fn test_collect_mq_commands_deduplicates_windows_extensions() {
    let temp_dir = std::env::temp_dir().join("mq-win-dedup-ext-test");
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    // Create the same subcommand with multiple Windows executable extensions
    fs::write(temp_dir.join("mq-foo.exe"), "test").expect("Failed to write file");
    fs::write(temp_dir.join("mq-foo.bat"), "@echo test").expect("Failed to write file");
    fs::write(temp_dir.join("mq-foo.cmd"), "@echo test").expect("Failed to write file");
    fs::write(temp_dir.join("mq-bar.exe"), "test").expect("Failed to write file");

    let mut seen = std::collections::HashSet::new();
    Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);

    assert_eq!(seen.len(), 2, "Should have exactly 2 unique commands");
    assert!(seen.contains("foo"), "Should contain 'foo'");
    assert!(seen.contains("bar"), "Should contain 'bar'");
}

/// Test that Windows deduplicates commands with different extensions across directories.
/// e.g., mq-foo.bat in dir1 and mq-foo.exe in dir2 should produce only "foo".
#[test]
#[cfg(windows)]
fn test_collect_mq_commands_deduplicates_across_dirs_windows() {
    let dir1 = std::env::temp_dir().join("mq-win-cross-dedup-1");
    let dir2 = std::env::temp_dir().join("mq-win-cross-dedup-2");
    fs::create_dir_all(&dir1).expect("Failed to create test directory");
    fs::create_dir_all(&dir2).expect("Failed to create test directory");

    defer! {
        if dir1.exists() {
            std::fs::remove_dir_all(&dir1).ok();
        }
        if dir2.exists() {
            std::fs::remove_dir_all(&dir2).ok();
        }
    }

    fs::write(dir1.join("mq-foo.bat"), "@echo test").expect("Failed to write file");
    fs::write(dir2.join("mq-foo.exe"), "test").expect("Failed to write file");
    fs::write(dir2.join("mq-unique.cmd"), "@echo test").expect("Failed to write file");

    let mut seen = std::collections::HashSet::new();
    Cli::collect_mq_commands_from_dir(&dir1, &mut seen);
    Cli::collect_mq_commands_from_dir(&dir2, &mut seen);

    assert_eq!(seen.len(), 2, "Should have exactly 2 unique commands");
    assert!(seen.contains("foo"), "Should contain 'foo'");
    assert!(seen.contains("unique"), "Should contain 'unique'");
}

/// Test that collect_mq_commands_from_dir handles an empty directory correctly.
#[test]
fn test_collect_mq_commands_from_empty_dir() {
    let temp_dir = std::env::temp_dir().join("mq-empty-dir-test");
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    let mut seen = std::collections::HashSet::new();
    Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);
    assert!(seen.is_empty(), "Empty directory should yield no commands");
}

/// Test that files without the mq- prefix are ignored even if executable.
#[test]
fn test_collect_mq_commands_ignores_non_mq_prefix() {
    let temp_dir = std::env::temp_dir().join("mq-prefix-test");
    fs::create_dir_all(&temp_dir).expect("Failed to create test directory");

    defer! {
        if temp_dir.exists() {
            std::fs::remove_dir_all(&temp_dir).ok();
        }
    }

    // Create files without mq- prefix
    fs::write(temp_dir.join("foo"), "test").expect("Failed to write file");
    fs::write(temp_dir.join("bar-mq"), "test").expect("Failed to write file");
    fs::write(temp_dir.join("mqfoo"), "test").expect("Failed to write file");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for name in &["foo", "bar-mq", "mqfoo"] {
            fs::set_permissions(temp_dir.join(name), fs::Permissions::from_mode(0o755))
                .expect("Failed to set permissions");
        }
    }

    let mut seen = std::collections::HashSet::new();
    Cli::collect_mq_commands_from_dir(&temp_dir, &mut seen);
    assert!(seen.is_empty(), "Files without mq- prefix should be ignored");
}

#[rstest]
#[case("md", InputFormat::Markdown)]
#[case("MD", InputFormat::Markdown)]
#[case("markdown", InputFormat::Markdown)]
#[case("mdx", InputFormat::Mdx)]
#[case("html", InputFormat::Html)]
#[case("htm", InputFormat::Html)]
#[case("txt", InputFormat::Raw)]
#[case("log", InputFormat::Raw)]
#[case("csv", InputFormat::Csv)]
#[case("gron", InputFormat::Gron)]
#[case("psv", InputFormat::Psv)]
#[case("tsv", InputFormat::Tsv)]
#[case("json", InputFormat::Json)]
#[case("toml", InputFormat::Toml)]
#[case("yaml", InputFormat::Yaml)]
#[case("yml", InputFormat::Yaml)]
#[case("xml", InputFormat::Xml)]
#[case("jsonl", InputFormat::Jsonl)]
#[case("ndjson", InputFormat::Jsonl)]
#[case("cbor", InputFormat::Cbor)]
#[case("unknown", InputFormat::Markdown)] // default fallback
fn test_from_extension(#[case] ext: &str, #[case] expected: InputFormat) {
    assert_eq!(InputFormat::from_extension(ext), expected);
}

#[rstest]
#[case("file.csv", InputFormat::Csv)]
#[case("file.csv.gz", InputFormat::Csv)]
#[case("file.json.gz", InputFormat::Json)]
#[case("file.tar.gz", InputFormat::Markdown)] // "tar" isn't a recognized inner extension
#[case("file.gz", InputFormat::Markdown)] // no inner extension at all
#[case("file.md", InputFormat::Markdown)]
fn test_input_format_from_path(#[case] filename: &str, #[case] expected: InputFormat) {
    assert_eq!(InputFormat::from_path(&PathBuf::from(filename)), expected);
}

#[rstest]
#[case("html", OutputFormat::Html)]
#[case("htm", OutputFormat::Html)]
#[case("txt", OutputFormat::Raw)]
#[case("log", OutputFormat::Raw)]
#[case("json", OutputFormat::Json)]
#[case("jsonl", OutputFormat::Jsonl)]
#[case("ndjson", OutputFormat::Jsonl)]
#[case("csv", OutputFormat::Csv)]
#[case("toml", OutputFormat::Toml)]
#[case("toon", OutputFormat::Toon)]
#[case("xml", OutputFormat::Xml)]
#[case("yaml", OutputFormat::Yaml)]
#[case("yml", OutputFormat::Yaml)]
#[case("gron", OutputFormat::Gron)]
#[case("unknown", OutputFormat::Markdown)] // default fallback
#[case("md", OutputFormat::Markdown)]
fn test_output_format_from_extension(#[case] ext: &str, #[case] expected: OutputFormat) {
    assert_eq!(OutputFormat::from_extension(ext), expected);
}

#[rstest]
#[case("file.json", OutputFormat::Json)]
#[case("file.csv", OutputFormat::Csv)]
#[case("file.yaml", OutputFormat::Yaml)]
#[case("file.md", OutputFormat::Markdown)]
#[case("file", OutputFormat::Markdown)] // no extension at all
fn test_output_format_from_path(#[case] filename: &str, #[case] expected: OutputFormat) {
    assert_eq!(OutputFormat::from_path(&PathBuf::from(filename)), expected);
}

#[rstest]
#[case("file.json", Some(r#"import "json" | json::json_parse()"#))]
#[case("file.gron", Some(r#"import "gron" | gron::gron_parse()"#))]
#[case("file.yaml", Some(r#"import "yaml" | yaml::yaml_parse()"#))]
#[case("file.yml", Some(r#"import "yaml" | yaml::yaml_parse()"#))]
#[case("file.toml", Some(r#"import "toml" | toml::toml_parse()"#))]
#[case("file.xml", Some(r#"import "xml" | xml::xml_parse()"#))]
#[case("file.toon", Some(r#"import "toon" | toon::toon_parse()"#))]
#[case("file.csv", Some(r#"import "csv" | csv::csv_parse(true)"#))]
#[case("file.tsv", Some(r#"import "csv" | csv::tsv_parse(true)"#))]
#[case("file.psv", Some(r#"import "csv" | csv::psv_parse(true)"#))]
#[case("file.cbor", Some(r#"import "cbor" | cbor::cbor_parse()"#))]
#[case("file.csv.gz", Some(r#"import "csv" | csv::csv_parse(true)"#))]
#[case("file.json.gz", Some(r#"import "json" | json::json_parse()"#))]
#[case("file.jsonl", Some(r#"import "json" | json::json_parse()"#))]
#[case("file.ndjson", Some(r#"import "json" | json::json_parse()"#))]
#[case("file.md", None)]
#[case("file.txt", None)]
fn test_auto_query_prefix(#[case] filename: &str, #[case] expected: Option<&str>) {
    let cli = Cli {
        input: InputArgs::default(),
        ..Cli::default()
    };
    let file = Some(PathBuf::from(filename));
    assert_eq!(cli.auto_query_prefix(&file).as_deref(), expected);
}

#[test]
fn test_auto_query_prefix_disabled_when_input_format_set() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Raw),
                ..Default::default()
            },
            ..Default::default()
        },
        ..Cli::default()
    };
    let file = Some(PathBuf::from("file.json"));
    assert_eq!(cli.auto_query_prefix(&file), None);
}

#[test]
fn test_auto_query_prefix_none_for_no_file() {
    let cli = Cli {
        input: InputArgs::default(),
        ..Cli::default()
    };
    assert_eq!(cli.auto_query_prefix(&None), None);
}

#[rstest]
#[case::none("file.md", false, "")]
#[case::aggregate("file.md", true, "nodes")]
#[case::format("file.json", false, r#"import "json" | json::json_parse()"#)]
#[case::format_then_aggregate("file.json", true, r#"import "json" | json::json_parse() | nodes"#)]
fn test_query_prelude(#[case] filename: &str, #[case] aggregate: bool, #[case] expected: &str) {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                aggregate,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Cli::default()
    };
    assert_eq!(cli.query_prelude(&Some(PathBuf::from(filename))), expected);
}

#[rstest]
#[case(InputFormat::Csv, None, false, r#"import "csv" | csv::csv_parse(true)"#)]
#[case(InputFormat::Csv, None, true, r#"import "csv" | csv::csv_parse(false)"#)]
#[case(
    InputFormat::Csv,
    Some(';'),
    false,
    r#"import "csv" | csv::csv_parse_with_delimiter(";", true)"#
)]
#[case(InputFormat::Tsv, None, true, r#"import "csv" | csv::tsv_parse(false)"#)]
#[case(InputFormat::Psv, None, true, r#"import "csv" | csv::psv_parse(false)"#)]
fn test_tabular_query_prefix(
    #[case] fmt: InputFormat,
    #[case] csv_delimiter: Option<char>,
    #[case] no_header: bool,
    #[case] expected: &str,
) {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(fmt.clone()),
                csv_delimiter,
                no_header,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Cli::default()
    };
    assert_eq!(cli.tabular_query_prefix(&fmt).as_deref(), Some(expected));
}

#[test]
fn test_tabular_query_prefix_none_for_non_tabular_format() {
    let cli = Cli {
        input: InputArgs::default(),
        ..Cli::default()
    };
    assert_eq!(cli.tabular_query_prefix(&InputFormat::Json), None);
}

fn frontmatter_cli(format: Option<InputFormat>, no_frontmatter: bool) -> Cli {
    Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: format,
                no_frontmatter,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Cli::default()
    }
}

#[rstest]
#[case::markdown(InputFormat::Markdown)]
#[case::mdx(InputFormat::Mdx)]
fn test_frontmatter_is_read_unless_turned_off(#[case] format: InputFormat) {
    let input = ContentData::Text("---\ntitle: x\n---\n\ntext".to_string());
    let names = |cli: &Cli| -> Vec<String> {
        cli.resolve_input(&None, &input)
            .unwrap()
            .iter()
            .map(|value| match value {
                mq_lang::RuntimeValue::Markdown(node, _) => node.name().to_string(),
                other => other.to_string(),
            })
            .collect()
    };

    assert_eq!(names(&frontmatter_cli(Some(format.clone()), false)), ["yaml", "text"]);
    assert_eq!(
        names(&frontmatter_cli(Some(format), true)),
        ["Horizontal_rule", "h2", "text"]
    );
}

#[rstest]
#[case::markdown(Some(InputFormat::Markdown), true)]
#[case::mdx(Some(InputFormat::Mdx), true)]
#[case::by_extension(None, true)]
#[case::csv(Some(InputFormat::Csv), false)]
#[case::json(Some(InputFormat::Json), false)]
fn test_no_frontmatter_applies_to_markdown_and_mdx(#[case] format: Option<InputFormat>, #[case] valid: bool) {
    assert_eq!(
        frontmatter_cli(format, true).validate_frontmatter_option().is_ok(),
        valid
    );
}

#[test]
fn test_no_frontmatter_is_not_checked_when_it_is_not_given() {
    assert!(
        frontmatter_cli(Some(InputFormat::Csv), false)
            .validate_frontmatter_option()
            .is_ok()
    );
}

#[test]
fn test_json_auto_parse() {
    let (_, temp_file_path) = create_file("auto_parse_test.json", r#"{"key": "value"}"#);
    let temp_file_path_clone = temp_file_path.clone();
    let (_, output_file) = create_file("auto_parse_output.md", "");
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(content.contains("value"), "JSON should be parsed automatically");
}

#[test]
fn test_csv_auto_parse() {
    let (_, temp_file_path) = create_file("auto_parse_test.csv", "name,age\nAlice,30\n");
    let temp_file_path_clone = temp_file_path.clone();
    let (_, output_file) = create_file("auto_parse_csv_output.md", "");
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(content.contains("Alice"), "CSV should be parsed automatically");
    assert!(content.contains("name"), "CSV header should be parsed");
}

#[test]
fn test_output_format_auto_detected_from_output_file_extension() {
    let (_, temp_file_path) = create_file("out_fmt_auto_input.md", "# Test");
    let (_, output_file) = create_file("out_fmt_auto_output.json", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    // No -F given; the `.json` extension on -o should drive the output format.
    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
    assert!(parsed.is_array(), "extension-inferred JSON output should be an array");
    assert_eq!(parsed[0]["type"], "Heading");
}

#[test]
fn test_output_format_explicit_flag_overrides_extension() {
    let (_, temp_file_path) = create_file("out_fmt_override_input.md", "# Test");
    let (_, output_file) = create_file("out_fmt_override_output.json", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    // -F yaml should win over the `.json` extension on -o.
    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Yaml),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(
        serde_json::from_str::<serde_json::Value>(&output_content).is_err(),
        "output should be YAML, not JSON"
    );
    assert!(output_content.contains("type: Heading") || output_content.contains("type: heading"));
}

#[test]
fn test_format_flag_sets_both_input_and_output() {
    // ".dat" isn't a recognized extension on either side, so without -T this
    // would parse as markdown and render as markdown.
    let (_, temp_file_path) = create_file("format_flag_input.dat", r#"{"key": "value"}"#);
    let (_, output_file) = create_file("format_flag_output.dat", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        format: Some(IoFormat::Json),
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    let parsed: serde_json::Value = serde_json::from_str(&output_content).expect("Output should be valid JSON");
    assert_eq!(parsed["key"], "value");
}

#[test]
fn test_format_flag_overridden_by_explicit_input_and_output_format() {
    let (_, temp_file_path) = create_file("format_flag_override_input.dat", "# Test");
    let (_, output_file) = create_file("format_flag_override_output.dat", "");
    let temp_file_path_clone = temp_file_path.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Markdown),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Table),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        format: Some(IoFormat::Json),
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let output_content = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(
        serde_json::from_str::<serde_json::Value>(&output_content).is_err(),
        "explicit -F table should win over -T json"
    );
}

#[test]
fn test_csv_delimiter_and_no_header_flags() {
    let (_, temp_file_path) = create_file("custom_delim_test.csv", "Alice;30\nBob;25\n");
    let temp_file_path_clone = temp_file_path.clone();
    let (_, output_file) = create_file("custom_delim_output.json", "");
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() {
            std::fs::remove_file(&temp_file_path_clone).ok();
        }
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Csv),
                csv_delimiter: Some(';'),
                no_header: true,
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Json),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        commands: None,
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let content = fs::read_to_string(&output_file).expect("Failed to read output");
    let json: serde_json::Value = serde_json::from_str(&content).expect("output should be valid JSON");
    // Headerless: every row is an array of values, not a dict keyed by a first-row header.
    assert_eq!(json, serde_json::json!([["Alice", "30"], ["Bob", "25"]]));
}

fn create_binary_file(name: &str, content: &[u8]) -> (PathBuf, PathBuf) {
    let temp_dir = std::env::temp_dir();
    let temp_file_path = temp_dir.join(name);
    let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
    file.write_all(content).expect("Failed to write to temp file");
    (temp_dir, temp_file_path)
}

#[test]
fn test_content_data_empty() {
    let c = ContentData::empty();
    assert_eq!(c.as_str(), Some(""));
    assert_eq!(c.as_bytes(), b"");
}

#[rstest]
#[case(ContentData::Text("hello".to_string()), Some("hello"))]
#[case(ContentData::Text("".to_string()), Some(""))]
#[case(ContentData::Bytes(vec![0xde, 0xad]), None)]
#[case(ContentData::Bytes(vec![]), None)]
fn test_content_data_as_str(#[case] input: ContentData, #[case] expected: Option<&str>) {
    assert_eq!(input.as_str(), expected);
}

#[rstest]
#[case(ContentData::Text("abc".to_string()), b"abc".as_ref())]
#[case(ContentData::Text("".to_string()), b"".as_ref())]
#[case(ContentData::Bytes(vec![0xde, 0xad, 0xbe, 0xef]), &[0xde, 0xad, 0xbe, 0xef])]
#[case(ContentData::Bytes(vec![]), b"".as_ref())]
fn test_content_data_as_bytes(#[case] input: ContentData, #[case] expected: &[u8]) {
    assert_eq!(input.as_bytes(), expected);
}

#[rstest]
#[case("hello".to_string(), Some("hello"))]
#[case("".to_string(), Some(""))]
fn test_content_data_from_string(#[case] s: String, #[case] expected_str: Option<&str>) {
    let c: ContentData = s.into();
    assert_eq!(c.as_str(), expected_str);
}

#[rstest]
#[case(vec![0x01, 0x02, 0x03])]
#[case(vec![])]
fn test_content_data_from_vec_u8(#[case] bytes: Vec<u8>) {
    let expected = bytes.clone();
    let c: ContentData = bytes.into();
    assert_eq!(c.as_str(), None);
    assert_eq!(c.as_bytes(), expected.as_slice());
}

#[rstest]
#[case(Some(InputFormat::Bytes), true)]
#[case(Some(InputFormat::Cbor), true)]
#[case(Some(InputFormat::Json), false)]
#[case(Some(InputFormat::Yaml), false)]
#[case(Some(InputFormat::Toml), false)]
#[case(Some(InputFormat::Markdown), false)]
#[case(Some(InputFormat::Raw), false)]
#[case(Some(InputFormat::Text), false)]
#[case(Some(InputFormat::Null), false)]
#[case(None, false)]
fn test_is_binary_format(#[case] fmt: Option<InputFormat>, #[case] expected: bool) {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: fmt,
                ..Default::default()
            },
            ..Default::default()
        },
        ..Cli::default()
    };
    assert_eq!(cli.is_binary_format(), expected);
}

#[rstest]
#[case(InputFormat::Bytes, None)]
#[case(InputFormat::Cbor, Some(r#"import "cbor" | cbor::cbor_parse()"#))]
#[case(InputFormat::Gron, Some(r#"import "gron" | gron::gron_parse()"#))]
#[case(InputFormat::Json, Some(r#"import "json" | json::json_parse()"#))]
#[case(InputFormat::Markdown, None)]
#[case(InputFormat::Raw, None)]
fn test_module_query_prefix(#[case] fmt: InputFormat, #[case] expected: Option<&str>) {
    assert_eq!(fmt.module_query_prefix(), expected);
}

#[rstest]
#[case(InputFormat::Bytes)]
#[case(InputFormat::Cbor)]
fn test_binary_format_streaming_returns_error(#[case] fmt: InputFormat) {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(fmt),
                ..Default::default()
            },
            stream: true,
            ..Default::default()
        },
        query: Some("self".to_string()),
        ..Cli::default()
    };
    assert!(cli.run().is_err());
}

#[rstest]
#[case(&[0x01, 0x02, 0x03, 0xff], "self", "bytes_self")]
#[case(&[0xca, 0xfe, 0xba, 0xbe], "self", "bytes_self2")]
fn test_bytes_input_self_roundtrip(#[case] data: &[u8], #[case] query: &str, #[case] suffix: &str) {
    let (_, temp_file_path) = create_binary_file(&format!("test_{suffix}.bin"), data);
    let temp_file_path_clone = temp_file_path.clone();
    let (_, output_file) = create_file(&format!("test_{suffix}_out.md"), "");
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() { std::fs::remove_file(&temp_file_path_clone).ok(); }
        if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Bytes),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some(query.to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read(&output_file).expect("Failed to read output");
    assert_eq!(result, data);
}

#[rstest]
#[case(&[0xca, 0xfe, 0xba, 0xbe], "4")]
#[case(&[0x01], "1")]
#[case(&[], "0")]
fn test_bytes_input_len(#[case] data: &[u8], #[case] expected_len: &str) {
    let suffix = format!("bytes_len_{}", data.len());
    let (_, temp_file_path) = create_binary_file(&format!("test_{suffix}.bin"), data);
    let temp_file_path_clone = temp_file_path.clone();
    let (_, output_file) = create_file(&format!("test_{suffix}_out.md"), "");
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() { std::fs::remove_file(&temp_file_path_clone).ok(); }
        if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Bytes),
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some("len()".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(result.trim(), expected_len);
}

#[rstest]
// CBOR text string "hello": major 3 (text), len 5
#[case(&[0x65, 0x68, 0x65, 0x6c, 0x6c, 0x6f], None, "hello", "cbor_auto_hello")]
// CBOR integer 42: 0x18 0x2a
#[case(&[0x18, 0x2a], Some(InputFormat::Cbor), "42", "cbor_explicit_42")]
// CBOR integer 0: 0x00
#[case(&[0x00], Some(InputFormat::Cbor), "0", "cbor_explicit_0")]
fn test_cbor_parse(
    #[case] cbor_bytes: &[u8],
    #[case] fmt: Option<InputFormat>,
    #[case] expected: &str,
    #[case] suffix: &str,
) {
    let ext = if fmt.is_none() { "cbor" } else { "bin" };
    let (_, temp_file_path) = create_binary_file(&format!("test_{suffix}.{ext}"), cbor_bytes);
    let temp_file_path_clone = temp_file_path.clone();
    let (_, output_file) = create_file(&format!("test_{suffix}_out.md"), "");
    let output_file_clone = output_file.clone();

    defer! {
        if temp_file_path_clone.exists() { std::fs::remove_file(&temp_file_path_clone).ok(); }
        if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: fmt,
                ..Default::default()
            },
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some("self".to_string()),
        files: Some(vec![temp_file_path]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(
        result.trim().contains(expected),
        "expected '{}' in output, got '{}'",
        expected,
        result.trim()
    );
}

#[rstest]
#[case::data_only(
        "data_only",
        None,
        Some(vec!["x".to_string(), "y".to_string(), "z".to_string()]),
        "ARGS",
        r#"{"positional": ["x", "y", "z"], "named": {}}"#
    )]
#[case::args_only(
        "args_only",
        Some(vec!["name".to_string(), "Alice".to_string()]),
        None,
        "ARGS",
        r#"{"positional": [], "named": {"name": "Alice"}}"#
    )]
#[case::args_and_data(
        "args_and_data",
        Some(vec!["name".to_string(), "Alice".to_string()]),
        Some(vec!["x".to_string(), "y".to_string()]),
        "ARGS",
        r#"{"positional": ["x", "y"], "named": {"name": "Alice"}}"#
    )]
#[case::positional_access(
        "positional_access",
        None,
        Some(vec!["a".to_string(), "b".to_string()]),
        r#"ARGS | ."positional""#,
        r#"["a", "b"]"#
    )]
#[case::named_access(
        "named_access",
        Some(vec!["key".to_string(), "val".to_string()]),
        None,
        r#"ARGS | ."named""#,
        r#"{"key": "val"}"#
    )]
#[case::named_individual_var(
        "named_individual_var",
        Some(vec!["greeting".to_string(), "hello".to_string()]),
        None,
        "greeting",
        "hello"
    )]
fn test_args_and_data(
    #[case] suffix: &str,
    #[case] arg: Option<Vec<String>>,
    #[case] args: Option<Vec<String>>,
    #[case] query: &str,
    #[case] expected: &str,
) {
    let (_, output_file) = create_file(&format!("test_args_data_{suffix}.md"), "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            arg,
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some(query.to_string()),
        args,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(result.trim(), expected, "query: {}", query);
}

#[rstest]
#[case::string_value(
        "string_value",
        vec!["name".to_string(), r#""Alice""#.to_string()],
        "name",
        "Alice"
    )]
#[case::number_value(
        "number_value",
        vec!["count".to_string(), "42".to_string()],
        "count",
        "42"
    )]
#[case::bool_value(
        "bool_value",
        vec!["flag".to_string(), "true".to_string()],
        "flag",
        "true"
    )]
#[case::null_value(
        "null_value",
        vec!["n".to_string(), "null".to_string()],
        "n",
        ""
    )]
#[case::array_value(
        "array_value",
        vec!["list".to_string(), "[1,2,3]".to_string()],
        "list",
        r#"[1, 2, 3]"#
    )]
#[case::object_value(
        "object_value",
        vec!["obj".to_string(), r#"{"a":1}"#.to_string()],
        "obj",
        r#"{"a": 1}"#
    )]
#[case::args_named_access(
        "args_named_access",
        vec!["count".to_string(), "42".to_string()],
        r#"ARGS | ."named""#,
        r#"{"count": 42}"#
    )]
fn test_argjson(#[case] suffix: &str, #[case] argjson: Vec<String>, #[case] query: &str, #[case] expected: &str) {
    let (_, output_file) = create_file(&format!("test_argjson_{suffix}.md"), "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            argjson: Some(argjson),
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some(query.to_string()),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(result.trim(), expected, "query: {}", query);
}

#[test]
fn test_argjson_invalid_json_errors() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            argjson: Some(vec!["name".to_string(), "not valid json".to_string()]),
            ..Default::default()
        },
        query: Some("name".to_string()),
        ..Cli::default()
    };

    assert!(
        cli.run().is_err(),
        "--argjson with malformed JSON should return an error"
    );
}

#[test]
fn test_argjson_combined_with_args_and_argv() {
    let (_, output_file) = create_file("test_argjson_combined.md", "");
    let output_file_clone = output_file.clone();

    defer! {
        if output_file_clone.exists() {
            std::fs::remove_file(&output_file_clone).ok();
        }
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            arg: Some(vec!["name".to_string(), "Alice".to_string()]),
            argjson: Some(vec!["count".to_string(), "42".to_string()]),
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some(r#"ARGS | ."named""#.to_string()),
        args: Some(vec!["x".to_string()]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    // Compare parsed JSON values (rather than the raw string) so key order doesn't matter.
    let actual: serde_json::Value = serde_json::from_str(result.trim()).expect("output should be valid JSON");
    let expected: serde_json::Value = serde_json::from_str(r#"{"count": 42, "name": "Alice"}"#).unwrap();
    assert_eq!(actual, expected);
}

#[rstest]
#[case::single_object("single_object", r#"{"a": 1}"#, "data", r#"[{"a": 1}]"#)]
#[case::single_array("single_array", "[1, 2, 3]", "data", r#"[[1, 2, 3]]"#)]
#[case::multiple_concatenated_values("multiple_values", "1 2 3", "data", "[1, 2, 3]")]
fn test_slurpfile(#[case] suffix: &str, #[case] file_content: &str, #[case] query: &str, #[case] expected: &str) {
    let (_, data_file) = create_file(&format!("test_slurpfile_{suffix}.json"), file_content);
    let (_, output_file) = create_file(&format!("test_slurpfile_{suffix}_out.md"), "");
    let data_file_clone = data_file.clone();
    let output_file_clone = output_file.clone();

    defer! {
        std::fs::remove_file(&data_file_clone).ok();
        std::fs::remove_file(&output_file_clone).ok();
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            slurp_file: Some(vec!["data".to_string(), data_file.to_string_lossy().to_string()]),
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some(query.to_string()),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(result.trim(), expected, "query: {}", query);
}

#[test]
fn test_slurpfile_invalid_json_errors() {
    let (_, data_file) = create_file("test_slurpfile_invalid.json", "not valid json");
    let data_file_clone = data_file.clone();

    defer! {
        std::fs::remove_file(&data_file_clone).ok();
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            slurp_file: Some(vec!["data".to_string(), data_file.to_string_lossy().to_string()]),
            ..Default::default()
        },
        query: Some("data".to_string()),
        ..Cli::default()
    };

    assert!(
        cli.run().is_err(),
        "--slurpfile with malformed JSON should return an error"
    );
}

#[test]
fn test_slurpfile_missing_file_errors() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            slurp_file: Some(vec!["data".to_string(), "/nonexistent/path/to/file.json".to_string()]),
            ..Default::default()
        },
        query: Some("data".to_string()),
        ..Cli::default()
    };

    assert!(
        cli.run().is_err(),
        "--slurpfile with a missing file should return an error"
    );
}

#[test]
fn test_missing_input_file_error_contains_path() {
    let cli = Cli {
        input: InputArgs::default(),
        query: Some(".h".to_string()),
        files: Some(vec![PathBuf::from("no_such_dir/missing_input.md")]),
        ..Cli::default()
    };

    let err = cli.run().expect_err("missing input file should return an error");
    assert!(
        err.to_string().contains("no_such_dir/missing_input.md"),
        "error should contain the file path, got: {err}"
    );
}

#[test]
fn test_duplicate_stdin_argument_errors() {
    let cli = Cli {
        query: Some(".h".to_string()),
        files: Some(vec![PathBuf::from("-"), PathBuf::from("-")]),
        ..Cli::default()
    };

    let err = cli.run().expect_err("`-` specified twice should return an error");
    assert!(err.to_string().contains("only once"), "unexpected error: {err}");
}

#[test]
fn test_repl_rejects_stdin_argument() {
    let cli = Cli {
        commands: Some(Commands::Repl {
            files: Some(vec![PathBuf::from("-")]),
        }),
        ..Cli::default()
    };

    let err = cli.run().expect_err("repl with `-` should return an error");
    assert!(err.to_string().contains("repl"), "unexpected error: {err}");
}

fn eval_file_vars(engine: &mut mq_lang::DefaultEngine) -> String {
    let compiled = engine.compile("__FILE__").unwrap();
    let result = engine
        .eval_compiled(&compiled, mq_lang::null_input().into_iter())
        .unwrap();
    result[0].to_string()
}

#[test]
fn test_set_file_vars_resets_for_stdin() {
    let cli = Cli::default();
    let mut engine = cli.create_engine().unwrap();
    let mut cache = ProgramCache::default();
    cli.set_file_vars(&mut engine, &mut cache, Some(Path::new("dir/a.md")));
    assert_eq!(eval_file_vars(&mut engine), "dir/a.md");
    cli.set_file_vars(&mut engine, &mut cache, None);
    assert_eq!(eval_file_vars(&mut engine), "");
}

#[test]
fn test_file_vars_default_to_empty_for_stdin() {
    let cli = Cli::default();
    let mut engine = cli.create_engine().unwrap();
    let mut cache = ProgramCache::default();
    cli.set_file_vars(&mut engine, &mut cache, None);
    assert_eq!(eval_file_vars(&mut engine), "");
}

#[test]
fn test_set_file_vars_keeps_user_global_for_stdin() {
    let cli = Cli {
        input: InputArgs {
            arg: Some(vec!["__FILE__".to_string(), "custom".to_string()]),
            ..Default::default()
        },
        ..Cli::default()
    };
    let mut engine = cli.create_engine().unwrap();
    let mut cache = ProgramCache::default();
    cli.set_file_vars(&mut engine, &mut cache, None);
    assert_eq!(eval_file_vars(&mut engine), "custom");
}

#[test]
fn test_slurpfile_combined_with_args_and_argjson() {
    let (_, data_file) = create_file("test_slurpfile_combined.json", r#"{"c": 3}"#);
    let (_, output_file) = create_file("test_slurpfile_combined_out.md", "");
    let data_file_clone = data_file.clone();
    let output_file_clone = output_file.clone();

    defer! {
        std::fs::remove_file(&data_file_clone).ok();
        std::fs::remove_file(&output_file_clone).ok();
    }

    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            arg: Some(vec!["name".to_string(), "Alice".to_string()]),
            argjson: Some(vec!["count".to_string(), "42".to_string()]),
            slurp_file: Some(vec!["data".to_string(), data_file.to_string_lossy().to_string()]),
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some("data".to_string()),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(result.trim(), r#"[{"c": 3}]"#, "query: data");
}

#[test]
fn test_files_without_data_single_file() {
    let (_, input_file) = create_file("test_files_no_data_single.md", "# hello");
    let (_, output_file) = create_file("test_files_no_data_single_out.md", "");
    let input_file_clone = input_file.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
        if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Text),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some("self".to_string()),
        files: Some(vec![input_file]),
        args: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(result.contains("hello"), "file content should be processed");
}

#[test]
fn test_files_without_data_multiple_files() {
    // Verify each file is processed independently; output_file is per-run so check each separately.
    let (_, file1) = create_file("test_files_no_data_multi1.md", "# file1");
    let (_, file2) = create_file("test_files_no_data_multi2.md", "# file2");
    let (_, out1) = create_file("test_files_no_data_multi_out1.md", "");
    let (_, out2) = create_file("test_files_no_data_multi_out2.md", "");
    let file1_clone = file1.clone();
    let file2_clone = file2.clone();
    let out1_clone = out1.clone();
    let out2_clone = out2.clone();

    defer! {
        if file1_clone.exists() { std::fs::remove_file(&file1_clone).ok(); }
        if file2_clone.exists() { std::fs::remove_file(&file2_clone).ok(); }
        if out1_clone.exists() { std::fs::remove_file(&out1_clone).ok(); }
        if out2_clone.exists() { std::fs::remove_file(&out2_clone).ok(); }
    }

    for (input, output, expected) in [(&file1, &out1, "file1"), (&file2, &out2, "file2")] {
        let cli = Cli {
            input: InputArgs::default(),
            output: OutputArgs {
                output_format: Some(OutputFormat::Text),
                output_file: Some(output.clone()),
                ..Default::default()
            },
            query: Some("self".to_string()),
            files: Some(vec![input.clone()]),
            args: None,
            ..Cli::default()
        };
        assert!(cli.run().is_ok());
        let result = fs::read_to_string(output).expect("Failed to read output");
        assert!(result.contains(expected), "file content '{}' should appear", expected);
    }

    // Also verify multi-file run (no output_file) succeeds without error
    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs::default(),
        query: Some("self".to_string()),
        files: Some(vec![file1, file2]),
        args: None,
        ..Cli::default()
    };
    assert!(cli.run().is_ok(), "multi-file run without --args should succeed");
}

#[test]
fn test_files_glob_pattern_expansion() {
    let (temp_dir, file1) = create_file("mq_glob_expand_test_a.md", "# File One");
    let (_, file2) = create_file("mq_glob_expand_test_b.md", "# File Two");
    let output_file = temp_dir.join("mq_glob_expand_result.md");
    let file1_clone = file1.clone();
    let file2_clone = file2.clone();
    let output_clone = output_file.clone();

    defer! {
        if file1_clone.exists() { std::fs::remove_file(&file1_clone).ok(); }
        if file2_clone.exists() { std::fs::remove_file(&file2_clone).ok(); }
        if output_clone.exists() { std::fs::remove_file(&output_clone).ok(); }
    }

    let pattern = temp_dir.join("mq_glob_expand_test_*.md");
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                aggregate: true,
                ..Default::default()
            },
            eval_all: true,
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Text),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some(".h | len".to_string()),
        files: Some(vec![pattern]),
        args: None,
        ..Cli::default()
    };

    assert!(cli.run().is_ok(), "glob pattern should expand and run successfully");
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(result.trim(), "2", "both files matched by the glob should be counted");
}

#[test]
fn test_files_multiple_glob_patterns_expansion() {
    let (temp_dir, file1) = create_file("mq_glob_multi_test_a.md", "# File One");
    let (_, file2) = create_file("mq_glob_multi_other_b.md", "# File Two");
    let output_file = temp_dir.join("mq_glob_multi_result.md");
    let file1_clone = file1.clone();
    let file2_clone = file2.clone();
    let output_clone = output_file.clone();

    defer! {
        if file1_clone.exists() { std::fs::remove_file(&file1_clone).ok(); }
        if file2_clone.exists() { std::fs::remove_file(&file2_clone).ok(); }
        if output_clone.exists() { std::fs::remove_file(&output_clone).ok(); }
    }

    let pattern1 = temp_dir.join("mq_glob_multi_test_*.md");
    let pattern2 = temp_dir.join("mq_glob_multi_other_*.md");
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                aggregate: true,
                ..Default::default()
            },
            eval_all: true,
            ..Default::default()
        },
        output: OutputArgs {
            output_format: Some(OutputFormat::Text),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some(".h | len".to_string()),
        files: Some(vec![pattern1, pattern2]),
        args: None,
        ..Cli::default()
    };

    assert!(
        cli.run().is_ok(),
        "multiple glob patterns should expand and run successfully"
    );
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert_eq!(
        result.trim(),
        "2",
        "files matched by both glob patterns should be counted"
    );
}

#[test]
fn test_files_glob_pattern_no_matches_falls_through_as_literal() {
    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs::default(),
        query: Some("self".to_string()),
        files: Some(vec![PathBuf::from("/nonexistent/mq_glob_no_match_*.md")]),
        args: None,
        ..Cli::default()
    };

    assert!(
        cli.run().is_err(),
        "non-matching glob pattern should surface a file error"
    );
}

#[test]
fn test_files_with_data_does_not_mix() {
    // --args values must not be treated as files, and files must not appear in ARGS
    let (_, input_file) = create_file("test_files_with_data.md", "# content");
    let (_, output_file) = create_file("test_files_with_data_out.md", "");
    let input_file_clone = input_file.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
        if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Raw),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some("ARGS".to_string()),
        files: Some(vec![input_file]),
        args: Some(vec!["alpha".to_string(), "beta".to_string()]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(result.contains("alpha"), "ARGS.positional should contain 'alpha'");
    assert!(result.contains("beta"), "ARGS.positional should contain 'beta'");
    assert!(!result.contains("content"), "file content must not appear in ARGS");
}

#[test]
fn test_files_without_data_args_undefined() {
    // Without --arg or --args, ARGS must be undefined (runtime error expected)
    let (_, input_file) = create_file("test_files_no_args_undefined.md", "# x");
    let input_file_clone = input_file.clone();

    defer! {
        if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs::default(),
        query: Some("ARGS".to_string()),
        files: Some(vec![input_file]),
        args: None,
        ..Cli::default()
    };

    assert!(
        cli.run().is_err(),
        "ARGS should be undefined when neither --arg nor --args is given"
    );
}

#[test]
fn test_files_with_data_file_content_processed() {
    // File content is still processed even when --args is given
    let (_, input_file) = create_file("test_files_data_content.md", "# heading");
    let (_, output_file) = create_file("test_files_data_content_out.md", "");
    let input_file_clone = input_file.clone();
    let output_file_clone = output_file.clone();

    defer! {
        if input_file_clone.exists() { std::fs::remove_file(&input_file_clone).ok(); }
        if output_file_clone.exists() { std::fs::remove_file(&output_file_clone).ok(); }
    }

    let cli = Cli {
        input: InputArgs::default(),
        output: OutputArgs {
            output_format: Some(OutputFormat::Text),
            output_file: Some(output_file.clone()),
            ..Default::default()
        },
        query: Some("self".to_string()),
        files: Some(vec![input_file]),
        args: Some(vec!["x".to_string()]),
        ..Cli::default()
    };

    assert!(cli.run().is_ok());
    let result = fs::read_to_string(&output_file).expect("Failed to read output");
    assert!(
        result.contains("heading"),
        "file content should still be processed when --args is given"
    );
}

#[test]
fn test_args_pair_works() {
    let cli = Cli {
        input: InputArgs {
            program: ProgramArgs {
                input_format: Some(InputFormat::Null),
                ..Default::default()
            },
            arg: Some(vec!["name".to_string(), "Alice".to_string()]),
            ..Default::default()
        },
        query: Some("name".to_string()),
        ..Cli::default()
    };
    assert!(cli.run().is_ok(), "--arg with a valid NAME VALUE pair should succeed");
}

#[cfg(feature = "watch")]
#[test]
fn test_watch_targets_requires_a_file() {
    let cli = Cli {
        files: None,
        query: Some("self".to_string()),
        ..Cli::default()
    };
    assert!(
        cli.watch_targets().is_err(),
        "--watch on stdin (no files) should be rejected"
    );
}

#[cfg(feature = "watch")]
#[test]
fn test_watch_targets_includes_input_files() {
    let file = PathBuf::from("input.md");
    let cli = Cli {
        files: Some(vec![file.clone()]),
        query: Some("self".to_string()),
        ..Cli::default()
    };
    assert_eq!(cli.watch_targets().unwrap(), vec![file]);
}

#[cfg(feature = "watch")]
#[test]
fn test_watch_targets_includes_query_file_when_from_file() {
    let input_file = PathBuf::from("input.md");
    let query_file = "query.mq";
    let cli = Cli {
        input: InputArgs {
            from_file: true,
            ..Default::default()
        },
        files: Some(vec![input_file.clone()]),
        query: Some(query_file.to_string()),
        ..Cli::default()
    };
    assert_eq!(
        cli.watch_targets().unwrap(),
        vec![input_file, PathBuf::from(query_file)]
    );
}

#[cfg(feature = "watch")]
#[rstest]
#[case(notify::EventKind::Create(notify::event::CreateKind::File), true)]
#[case(notify::EventKind::Modify(notify::event::ModifyKind::Any), true)]
#[case(notify::EventKind::Remove(notify::event::RemoveKind::File), true)]
#[case(notify::EventKind::Access(notify::event::AccessKind::Read), false)]
#[case(
    notify::EventKind::Access(notify::event::AccessKind::Close(notify::event::AccessMode::Write)),
    false
)]
#[case(notify::EventKind::Other, false)]
#[case(notify::EventKind::Any, false)]
fn test_is_content_change(#[case] kind: notify::EventKind, #[case] expected: bool) {
    assert_eq!(Cli::is_content_change(&kind), expected);
}

#[test]
fn test_directory_expands_to_sorted_markdown_files() {
    let dir = tempfile::tempdir().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(dir.path().join("b.md"), "# B").unwrap();
    fs::write(sub.join("a.mdx"), "# A").unwrap();
    fs::write(dir.path().join("note.txt"), "skip").unwrap();

    let files = Cli::expand_glob_patterns(&[dir.path().to_path_buf()]).unwrap();

    assert_eq!(files, vec![dir.path().join("b.md"), sub.join("a.mdx")]);
}

#[test]
fn test_directory_without_markdown_files_is_error() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.txt"), "skip").unwrap();

    assert!(Cli::expand_glob_patterns(&[dir.path().to_path_buf()]).is_err());
}
