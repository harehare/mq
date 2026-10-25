use super::*;
use rstest::rstest;

#[rstest]
#[case("@test", Some(TestAnnotation::Test))]
#[case("  @test  ", Some(TestAnnotation::Test))]
#[case("[test]", Some(TestAnnotation::Test))]
#[case("  [test]  ", Some(TestAnnotation::Test))]
#[case(
        "@parametrize([[1, 2], [3, 4]])",
        Some(TestAnnotation::Parametrize { params_expr: "[[1, 2], [3, 4]]".to_string() })
    )]
#[case(
        "  @parametrize(  [[1, 2]]  )  ",
        Some(TestAnnotation::Parametrize { params_expr: "[[1, 2]]".to_string() })
    )]
#[case(
        "@parametrize(range(0, 5))",
        Some(TestAnnotation::Parametrize { params_expr: "range(0, 5)".to_string() })
    )]
#[case(
        "@parametrize([])",
        Some(TestAnnotation::Parametrize { params_expr: "[]".to_string() })
    )]
#[case(
        "@tags(slow, integration)",
        Some(TestAnnotation::Tags(vec!["slow".to_string(), "integration".to_string()]))
    )]
#[case(
        "  @tags(  slow ,  integration  )  ",
        Some(TestAnnotation::Tags(vec!["slow".to_string(), "integration".to_string()]))
    )]
#[case("@tag(slow)", Some(TestAnnotation::Tags(vec!["slow".to_string()])))]
#[case("@tags()", Some(TestAnnotation::Tags(vec![])))]
#[case(
        "@property(100, [gen::int(0, 10)])",
        Some(TestAnnotation::Property {
            count_expr: "100".to_string(),
            generators_expr: "[gen::int(0, 10)]".to_string(),
        })
    )]
#[case(
        "@property([gen::int(0, 10)])",
        Some(TestAnnotation::Property {
            count_expr: DEFAULT_PROPERTY_CASES.to_string(),
            generators_expr: "[gen::int(0, 10)]".to_string(),
        })
    )]
#[case(
        "  @property(  50 ,  [gen::int(0, 10)]  )  ",
        Some(TestAnnotation::Property {
            count_expr: "50".to_string(),
            generators_expr: "[gen::int(0, 10)]".to_string(),
        })
    )]
#[case(
        // A comma inside a nested string/array must not be mistaken for the count/generators
        // separator.
        "@property(10, [gen::int(0, 10), gen::string(\"a,b\", 3)])",
        Some(TestAnnotation::Property {
            count_expr: "10".to_string(),
            generators_expr: "[gen::int(0, 10), gen::string(\"a,b\", 3)]".to_string(),
        })
    )]
#[case(
        // `]` inside a string must not fool the bracket-depth scanner into treating the next
        // comma as the count/generators separator.
        "@property([gen::string(\"]\", 3), gen::int(0, 1)])",
        Some(TestAnnotation::Property {
            count_expr: DEFAULT_PROPERTY_CASES.to_string(),
            generators_expr: "[gen::string(\"]\", 3), gen::int(0, 1)]".to_string(),
        })
    )]
#[case("@unknown(foo)", None)]
#[case("@skip", None)]
#[case("not an annotation", None)]
#[case("@", None)]
#[case("@parametrize", None)]
#[case("@property", None)]
fn test_parse_annotation(#[case] input: &str, #[case] expected: Option<TestAnnotation>) {
    assert_eq!(TestRunner::parse_annotation(input), expected);
}

#[rstest]
#[case("100, [gen::int(0, 10)]", Some(("100", " [gen::int(0, 10)]")))]
#[case("[gen::int(0, 10)]", None)]
#[case(
        "10, [gen::int(0, 10), gen::string(\"a,b\", 3)]",
        Some(("10", " [gen::int(0, 10), gen::string(\"a,b\", 3)]"))
    )]
#[case("", None)]
fn test_split_top_level_comma(#[case] input: &str, #[case] expected: Option<(&str, &str)>) {
    assert_eq!(TestRunner::split_top_level_comma(input), expected);
}

#[test]
fn test_build_property_case_expr() {
    let case_expr = TestRunner::build_property_case_expr("range", "test_range", "100", "[gen::int(0, 10)]", 1);
    assert!(
        case_expr.contains("let __property_generators = ([gen::int(0, 10)])"),
        "missing generators binding: {case_expr}"
    );
    assert!(
        case_expr.contains("len(__property_generators) != 1"),
        "missing arity check: {case_expr}"
    );
    assert!(
        case_expr.contains("let __property_count = (100)"),
        "missing count binding: {case_expr}"
    );
    assert!(
        case_expr.contains("__property_count <= 0"),
        "missing zero-count guard: {case_expr}"
    );
    assert!(
        case_expr.contains("range(0, __property_count - 1)"),
        "missing count range: {case_expr}"
    );
    assert!(
        case_expr.contains("(gen::tuple(__property_generators))(__property_seed)"),
        "missing generator call: {case_expr}"
    );
    assert!(
        case_expr.contains("test_range(__property_arg[0])"),
        "missing unpacked call: {case_expr}"
    );
    assert!(
        case_expr.contains("_property_case_result(__property_args,"),
        "missing shrink wiring: {case_expr}"
    );
    assert!(case_expr.contains("\"range[\""), "missing display label: {case_expr}");
}

#[test]
fn test_merge_annotation_comments_joins_a_multiline_parametrize_block() {
    let content = concat!(
        "# @tags(slow)\n",
        "# @parametrize([\n",
        "# [1, 2],\n",
        "# [3, 4]\n",
        "# ])\n",
        "def test_add(a, b):\n  None\nend\n",
    );
    let node = first_def(content);

    let merged = TestRunner::merge_annotation_comments(&node.leading_trivia);

    assert_eq!(
        merged,
        vec![
            " @tags(slow)".to_string(),
            " @parametrize([\n [1, 2],\n [3, 4]\n ])".to_string(),
        ]
    );
}

#[test]
fn test_merge_annotation_comments_leaves_single_line_annotations_untouched() {
    let content = "# @test\n# @tags(slow)\ndef my_check():\n  None\nend\n";
    let node = first_def(content);

    let merged = TestRunner::merge_annotation_comments(&node.leading_trivia);

    assert_eq!(merged, vec![" @test".to_string(), " @tags(slow)".to_string()]);
}

#[test]
fn test_merge_annotation_comments_ignores_unmatched_parens_in_plain_comments() {
    let content = concat!(
        "# Explanatory note with an unmatched paren (like this\n",
        "# that spans lines without closing it.\n",
        "# @parametrize([[1, 2], [3, 4]])\n",
        "def test_add(a, b):\n  None\nend\n",
    );
    let node = first_def(content);

    let merged = TestRunner::merge_annotation_comments(&node.leading_trivia);

    assert_eq!(
        merged,
        vec![
            " Explanatory note with an unmatched paren (like this".to_string(),
            " that spans lines without closing it.".to_string(),
            " @parametrize([[1, 2], [3, 4]])".to_string(),
        ]
    );
}

fn first_def(content: &str) -> mq_lang::Shared<mq_lang::CstNode> {
    let (nodes, _) = mq_lang::parse_recovery(content);
    nodes
        .into_iter()
        .find(|n| matches!(n.kind, mq_lang::CstNodeKind::Def { .. }))
        .expect("no def node found")
}

#[rstest]
#[case("def foo():\n  None\nend\n", 0)]
#[case("def foo(x):\n  None\nend\n", 1)]
#[case("def foo(x, y):\n  None\nend\n", 2)]
#[case("def foo(x, y, z):\n  None\nend\n", 3)]
#[case("def foo(a, b, c, d):\n  None\nend\n", 4)]
fn test_get_arity(#[case] content: &str, #[case] expected: usize) {
    let node = first_def(content);
    assert_eq!(TestRunner::get_arity(&node), expected);
}

#[rstest]
#[case(
        "def test_foo():\n  None\nend\n\ndef helper():\n  None\nend\n\ndef test_bar():\n  None\nend\n",
        vec![
            DiscoveredTest::Simple { name: "test_foo".to_string(), tags: vec![] },
            DiscoveredTest::Simple { name: "test_bar".to_string(), tags: vec![] },
        ]
    )]
#[case(
        "# @test\ndef my_check():\n  None\nend\n\ndef not_a_test():\n  None\nend\n",
        vec![DiscoveredTest::Simple { name: "my_check".to_string(), tags: vec![] }]
    )]
#[case(
        "# [test]\ndef another_check():\n  None\nend\n",
        vec![DiscoveredTest::Simple { name: "another_check".to_string(), tags: vec![] }]
    )]
#[case(
        "def test_first():\n  None\nend\n\n# @test\ndef annotated():\n  None\nend\n",
        vec![
            DiscoveredTest::Simple { name: "test_first".to_string(), tags: vec![] },
            DiscoveredTest::Simple { name: "annotated".to_string(), tags: vec![] },
        ]
    )]
#[case("def helper():\n  None\nend\n", vec![])]
#[case(
        "module a:\n  def test_first():\n  None\nend\n\n# @test\ndef annotated():\n  None\nend\nend\n",
        vec![
            DiscoveredTest::Simple { name: "test_first".to_string(), tags: vec![] },
            DiscoveredTest::Simple { name: "annotated".to_string(), tags: vec![] },
        ]
    )]
#[case(
        "# @tags(slow, integration)\n# @test\ndef my_check():\n  None\nend\n",
        vec![DiscoveredTest::Simple {
            name: "my_check".to_string(),
            tags: vec!["slow".to_string(), "integration".to_string()],
        }]
    )]
#[case(
        "# @tags(slow)\ndef test_first():\n  None\nend\n",
        vec![DiscoveredTest::Simple { name: "test_first".to_string(), tags: vec!["slow".to_string()] }]
    )]
fn test_discover_tests_simple(#[case] content: &str, #[case] expected: Vec<DiscoveredTest>) {
    assert_eq!(TestRunner::discover_tests(content), expected);
}

#[rstest]
#[case(
    "# @parametrize([[\"hello\", 5], [\"world\", 5]])\ndef test_len(input, expected):\n  None\nend\n",
    "test_len",
    "[[\"hello\", 5], [\"world\", 5]]",
    2
)]
#[case(
    "# @parametrize([[1], [2], [3]])\ndef test_double(x):\n  None\nend\n",
    "test_double",
    "[[1], [2], [3]]",
    1
)]
#[case(
    "# @parametrize([[\"a\", \"b\", \"ab\"], [\"x\", \"y\", \"xy\"]])\ndef test_concat(a, b, expected):\n  None\nend\n",
    "test_concat",
    "[[\"a\", \"b\", \"ab\"], [\"x\", \"y\", \"xy\"]]",
    3
)]
#[case(
    "# @parametrize([[\"ignored\"]])\ndef test_no_args():\n  None\nend\n",
    "test_no_args",
    "[[\"ignored\"]]",
    0
)]
fn test_discover_tests_parametrized(
    #[case] content: &str,
    #[case] expected_name: &str,
    #[case] expected_params_expr: &str,
    #[case] expected_arity: usize,
) {
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 1);
    match &tests[0] {
        DiscoveredTest::Parametrized {
            name,
            params_expr,
            arity,
            ..
        } => {
            assert_eq!(name, expected_name);
            assert_eq!(params_expr, expected_params_expr);
            assert_eq!(*arity, expected_arity);
        }
        other => panic!("expected Parametrized, got {other:?}"),
    }
}

#[test]
fn test_discover_tests_property_is_discovered_as_a_property_test() {
    let content = "# @property(50, [gen::int(0, 10)])\ndef test_range(n):\n  None\nend\n";
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 1);
    match &tests[0] {
        DiscoveredTest::Property {
            name,
            count_expr,
            generators_expr,
            arity,
            ..
        } => {
            assert_eq!(name, "test_range");
            assert_eq!(*arity, 1);
            assert_eq!(count_expr, "50");
            assert_eq!(generators_expr, "[gen::int(0, 10)]");
        }
        other => panic!("expected Property, got {other:?}"),
    }
}

#[test]
fn test_discover_tests_property_without_count_uses_the_default() {
    let content = "# @property([gen::int(0, 10)])\ndef test_range(n):\n  None\nend\n";
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 1);
    assert!(matches!(
        &tests[0],
        DiscoveredTest::Property { count_expr, .. }
            if count_expr == DEFAULT_PROPERTY_CASES
    ));
}

#[rstest]
#[case(
        concat!(
            "# @parametrize([\n",
            "# [\"hello\", 5],\n",
            "# [\"world\", 5],\n",
            "# ])\n",
            "def test_len(input, expected):\n  None\nend\n",
        ),
        "test_len",
        2
    )]
#[case(
        concat!(
            "# @parametrize(\n",
            "#   [[1, 2], [3, 4]]\n",
            "# )\n",
            "def test_add(a, b):\n  None\nend\n",
        ),
        "test_add",
        2
    )]
#[case(
        concat!(
            "# @tags(slow)\n",
            "# @parametrize([\n",
            "# [1, 2],\n",
            "# [3, 4]\n",
            "# ])\n",
            "def test_add(a, b):\n  None\nend\n",
        ),
        "test_add",
        2
    )]
fn test_discover_tests_parametrized_multiline(
    #[case] content: &str,
    #[case] expected_name: &str,
    #[case] expected_arity: usize,
) {
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 1);
    match &tests[0] {
        DiscoveredTest::Parametrized { name, arity, .. } => {
            assert_eq!(name, expected_name);
            assert_eq!(*arity, expected_arity);
        }
        other => panic!("expected Parametrized, got {other:?}"),
    }
}

#[test]
fn test_discover_tests_multiple_parametrized() {
    let content = concat!(
        "# @parametrize([[1, 2], [3, 4]])\n",
        "def test_add(a, b):\n  None\nend\n\n",
        "# @parametrize([[\"hello\", 5]])\n",
        "def test_len(s, n):\n  None\nend\n",
    );
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 2);
    assert!(matches!(&tests[0], DiscoveredTest::Parametrized { name, .. } if name == "test_add"));
    assert!(matches!(&tests[1], DiscoveredTest::Parametrized { name, .. } if name == "test_len"));
}

#[test]
fn test_discover_tests_mixed_all_kinds() {
    let content = concat!(
        "def test_simple():\n  None\nend\n\n",
        "# @test\ndef annotated():\n  None\nend\n\n",
        "# @parametrize([[1, 2]])\ndef test_param(a, b):\n  None\nend\n",
    );
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 3);
    assert!(matches!(&tests[0], DiscoveredTest::Simple { name, .. } if name == "test_simple"));
    assert!(matches!(&tests[1], DiscoveredTest::Simple { name, .. } if name == "annotated"));
    assert!(matches!(&tests[2], DiscoveredTest::Parametrized { name, .. } if name == "test_param"));
}

#[test]
fn test_discover_tests_parametrized_in_module() {
    let content = concat!(
        "module m:\n",
        "  # @parametrize([[1, 2]])\n",
        "  def test_add(a, b):\n  None\nend\n",
        "end\n",
    );
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 1);
    assert!(matches!(&tests[0], DiscoveredTest::Parametrized { name, arity, .. } if name == "test_add" && *arity == 2));
}

#[test]
fn test_discover_tests_ignores_unknown_annotation() {
    let content = "# @skip\ndef my_check():\n  None\nend\n";
    let tests = TestRunner::discover_tests(content);
    assert!(tests.is_empty());
}

#[rstest]
#[case(vec![DiscoveredTest::Simple { name: "test_foo".to_string(), tags: vec![] }], "[test_case(\"foo\", test_foo)]")]
#[case(vec![DiscoveredTest::Simple { name: "test_is_array".to_string(), tags: vec![] }], "[test_case(\"is_array\", test_is_array)]")]
#[case(vec![DiscoveredTest::Simple { name: "my_check".to_string(), tags: vec![] }], "[test_case(\"my_check\", my_check)]")]
fn test_build_test_query_simple_cases(#[case] tests: Vec<DiscoveredTest>, #[case] expected: &str) {
    let query = TestRunner::build_test_query("content", &tests);
    assert!(query.starts_with("content\n"), "query must start with original content");
    assert!(query.contains("flatten(["), "must use flatten");
    assert!(query.contains(expected), "expected {expected:?} in:\n{query}");
}

#[rstest]
#[case(
        DiscoveredTest::Parametrized { name: "test_no_args".to_string(), params_expr: "[[]]".to_string(), arity: 0, tags: vec![] },
        "test_no_args()",
        "\"no_args[\""
    )]
#[case(
        DiscoveredTest::Parametrized { name: "test_double".to_string(), params_expr: "[[1], [2]]".to_string(), arity: 1, tags: vec![] },
        "test_double(__ic[1][0])",
        "\"double[\""
    )]
#[case(
        DiscoveredTest::Parametrized { name: "test_len".to_string(), params_expr: "[[\"a\", 1]]".to_string(), arity: 2, tags: vec![] },
        "test_len(__ic[1][0], __ic[1][1])",
        "\"len[\""
    )]
#[case(
        DiscoveredTest::Parametrized { name: "test_concat".to_string(), params_expr: "[[\"a\", \"b\", \"ab\"]]".to_string(), arity: 3, tags: vec![] },
        "test_concat(__ic[1][0], __ic[1][1], __ic[1][2])",
        "\"concat[\""
    )]
#[case(
        DiscoveredTest::Parametrized { name: "check_len".to_string(), params_expr: "[[1]]".to_string(), arity: 1, tags: vec![] },
        "check_len(__ic[1][0])",
        "\"check_len[\""
    )]
fn test_build_test_query_parametrized_cases(
    #[case] test: DiscoveredTest,
    #[case] expected_call: &str,
    #[case] expected_label: &str,
) {
    let query = TestRunner::build_test_query("content", &[test]);
    assert!(query.contains("flatten(["), "must use flatten");
    assert!(query.contains("map("), "must use map");
    assert!(query.contains("zip(range("), "must use zip+range");
    assert!(
        query.contains(expected_call),
        "expected call {expected_call:?} in:\n{query}"
    );
    assert!(
        query.contains(expected_label),
        "expected label {expected_label:?} in:\n{query}"
    );
}

#[test]
fn test_build_test_query_multiple_simple() {
    let tests = vec![
        DiscoveredTest::Simple {
            name: "test_foo".to_string(),
            tags: vec![],
        },
        DiscoveredTest::Simple {
            name: "test_bar".to_string(),
            tags: vec![],
        },
    ];
    let query = TestRunner::build_test_query("content", &tests);
    assert!(query.contains("[test_case(\"foo\", test_foo)]"));
    assert!(query.contains("[test_case(\"bar\", test_bar)]"));
    assert!(query.contains("flatten(["));
}

#[test]
fn test_build_test_query_mixed() {
    let tests = vec![
        DiscoveredTest::Simple {
            name: "test_foo".to_string(),
            tags: vec![],
        },
        DiscoveredTest::Parametrized {
            name: "test_len".to_string(),
            params_expr: "[[\"a\", 1]]".to_string(),
            arity: 2,
            tags: vec![],
        },
    ];
    let query = TestRunner::build_test_query("content", &tests);
    assert!(query.contains("[test_case(\"foo\", test_foo)]"));
    assert!(query.contains("map("));
    assert!(query.contains("test_len(__ic[1][0], __ic[1][1])"));
    assert!(query.contains("flatten(["));
}

#[test]
fn test_build_test_query_preserves_content() {
    let content = "include \"test\"\n|\ndef helper(): None end";
    let tests = vec![DiscoveredTest::Simple {
        name: "test_foo".to_string(),
        tags: vec![],
    }];
    let query = TestRunner::build_test_query(content, &tests);
    assert!(query.starts_with(content));
}

#[test]
fn test_build_test_query_with_fn_uses_the_given_report_function() {
    let tests = vec![DiscoveredTest::Simple {
        name: "test_foo".to_string(),
        tags: vec![],
    }];
    let query = TestRunner::build_test_query_with_fn("content", &tests, "run_tests_data");
    assert!(query.contains("| run_tests_data(flatten(["));
    assert!(!query.contains("| run_tests(flatten(["));
}

#[rstest]
#[case(DiscoveredTest::Simple { name: "test_foo".to_string(), tags: vec![] }, "simple")]
#[case(DiscoveredTest::Parametrized { name: "test_foo".to_string(), params_expr: "[]".to_string(), arity: 1, tags: vec![] }, "parametrized")]
#[case(DiscoveredTest::Property { name: "test_foo".to_string(), count_expr: "10".to_string(), generators_expr: "[]".to_string(), arity: 1, tags: vec![] }, "property")]
fn test_discovered_test_kind(#[case] test: DiscoveredTest, #[case] expected: &str) {
    assert_eq!(test.kind(), expected);
}

#[test]
fn test_format_list_report_text_lists_files_tests_kind_and_tags() {
    let discovered = vec![(
        PathBuf::from("tests.mq"),
        vec![
            DiscoveredTest::Simple {
                name: "test_foo".to_string(),
                tags: vec!["smoke".to_string()],
            },
            DiscoveredTest::Parametrized {
                name: "test_bar".to_string(),
                params_expr: "[]".to_string(),
                arity: 1,
                tags: vec![],
            },
        ],
    )];

    let report = TestRunner::format_list_report(&discovered, OutputFormat::Text);

    assert_eq!(report, "tests.mq\n  foo (simple) [smoke]\n  bar (parametrized)");
}

#[test]
fn test_format_list_report_json_matches_the_extension_facing_shape() {
    let discovered = vec![(
        PathBuf::from("tests.mq"),
        vec![DiscoveredTest::Simple {
            name: "test_foo".to_string(),
            tags: vec!["smoke".to_string()],
        }],
    )];

    let report = TestRunner::format_list_report(&discovered, OutputFormat::Json);
    let parsed: serde_json::Value = serde_json::from_str(&report).expect("valid json");

    assert_eq!(
        parsed,
        serde_json::json!({
            "files": [{
                "file": "tests.mq",
                "tests": [{"name": "foo", "kind": "simple", "tags": ["smoke"]}],
            }]
        })
    );
}

#[test]
fn test_format_run_json_report_wraps_per_file_results() {
    let results = vec![serde_json::json!({
        "file": "tests.mq",
        "tests": [{"name": "foo", "status": "passed", "duration": 1, "error": null}],
    })];

    let report = TestRunner::format_run_json_report(results.clone());
    let parsed: serde_json::Value = serde_json::from_str(&report).expect("valid json");

    assert_eq!(parsed, serde_json::json!({ "files": results }));
}

#[test]
fn test_run_list_does_not_execute_tests_and_always_succeeds() {
    let dir = temp_project_dir("list_does_not_execute");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "|\n",
            "def test_would_fail():\n",
            "  assert(false)\n",
            "end\n",
        ),
    )
    .unwrap();

    let ok = TestRunner::new(vec![test_file]).with_list(true).run().unwrap();
    assert!(ok);

    fs::remove_dir_all(&dir).ok();
}

#[rstest]
#[case("macos", "open", Vec::<&str>::new())]
#[case("windows", "cmd", vec!["/C", "start", ""])]
#[case("linux", "xdg-open", Vec::<&str>::new())]
#[case("freebsd", "xdg-open", Vec::<&str>::new())]
fn test_build_open_command(#[case] target_os: &str, #[case] expected_program: &str, #[case] expected_args: Vec<&str>) {
    let path = PathBuf::from("coverage.html");
    let cmd = build_open_command(&path, target_os);

    assert_eq!(cmd.get_program(), expected_program);

    let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().to_string()).collect();
    let mut expected: Vec<String> = expected_args.into_iter().map(String::from).collect();
    expected.push("coverage.html".to_string());
    assert_eq!(args, expected);
}

#[rstest]
#[case(None, vec![], "test_foo", vec![], true)]
#[case(Some("foo"), vec![], "test_foo", vec![], true)]
#[case(Some("FOO"), vec![], "test_foo", vec![], true)]
#[case(Some("bar"), vec![], "test_foo", vec![], false)]
#[case(None, vec!["slow".to_string()], "test_foo", vec!["slow".to_string()], true)]
#[case(None, vec!["slow".to_string()], "test_foo", vec!["fast".to_string()], false)]
#[case(None, vec!["slow".to_string()], "test_foo", vec![], false)]
#[case(Some("foo"), vec!["slow".to_string()], "test_foo", vec!["slow".to_string()], true)]
#[case(Some("bar"), vec!["slow".to_string()], "test_foo", vec!["slow".to_string()], false)]
fn test_matches(
    #[case] filter: Option<&str>,
    #[case] tags: Vec<String>,
    #[case] test_name: &str,
    #[case] test_tags: Vec<String>,
    #[case] expected: bool,
) {
    let runner = TestRunner::new(vec![])
        .with_filter(filter.map(str::to_string))
        .with_tags(tags);
    let test = DiscoveredTest::Simple {
        name: test_name.to_string(),
        tags: test_tags,
    };
    assert_eq!(runner.matches(&test), expected);
}

fn temp_project_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mq_test_{name}_{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Writes a minimal test file exercising `assert_snapshot("greeting", "hello world")`,
/// shared by the `assert_snapshot` integration tests below.
fn write_greeting_snapshot_test_file(dir: &Path) -> PathBuf {
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        "include \"test\"\n|\ndef test_greeting():\n  assert_snapshot(\"greeting\", \"hello world\")\nend\n",
    )
    .unwrap();
    test_file
}

#[test]
fn test_run_executes_a_multiline_parametrize_block() {
    let dir = temp_project_dir("multiline_parametrize");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "|\n",
            "# @parametrize([\n",
            "#   [\"hello\", 5],\n",
            "#   [\"world\", 5],\n",
            "#   [\"\", 0],\n",
            "# ])\n",
            "def test_len(input, expected):\n",
            "  assert_eq(len(input), expected)\n",
            "end\n",
        ),
    )
    .unwrap();

    assert!(TestRunner::new(vec![test_file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_run_executes_a_property_test_over_generated_cases() {
    let dir = temp_project_dir("property_commutative");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "| import \"gen\"\n",
            "|\n",
            "# @property(30, [gen::int(-1000, 1000), gen::int(-1000, 1000)])\n",
            "def test_addition_is_commutative(a, b):\n",
            "  assert_eq(a + b, b + a)\n",
            "end\n",
        ),
    )
    .unwrap();

    assert!(TestRunner::new(vec![test_file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_run_a_property_test_without_a_count_defaults_to_100_cases() {
    let dir = temp_project_dir("property_default_count");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "| import \"gen\"\n",
            "|\n",
            "# @property([gen::int(0, 1000)])\n",
            "def test_is_non_negative(n):\n",
            "  assert_true(n >= 0)\n",
            "end\n",
        ),
    )
    .unwrap();

    assert!(TestRunner::new(vec![test_file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_a_failing_property_case_is_labeled_with_its_seed_and_fails_the_run() {
    let dir = temp_project_dir("property_failure");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "| import \"gen\"\n",
            "|\n",
            "# A deliberately false property: every generated int is < 5.\n",
            "# @property(20, [gen::int(0, 20)])\n",
            "def test_deliberately_false(n):\n",
            "  assert_true(n < 5)\n",
            "end\n",
        ),
    )
    .unwrap();

    assert!(
        !TestRunner::new(vec![test_file]).run().unwrap(),
        "a genuinely false property must fail the run"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_a_property_with_too_few_generators_fails_the_run() {
    let dir = temp_project_dir("property_generator_arity_mismatch");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "| import \"gen\"\n",
            "|\n",
            "# @property([gen::const(1)])\n",
            "def test_pair(a, b):\n",
            "  assert_true(is_none(b))\n",
            "end\n",
        ),
    )
    .unwrap();

    assert!(
        !TestRunner::new(vec![test_file]).run().unwrap(),
        "one generator for two parameters must fail, not silently pass b = None"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_a_failing_property_case_is_shrunk_to_a_minimal_counterexample() {
    // Calls `_property_case_result` (from `test.mq`) directly with args `[500]`, bypassing
    // `gen::tuple` entirely so the starting point is deterministic: the smallest `n` for
    // which `n < 5` is false is `5`, so a working shrink search must report exactly that.
    let query = concat!(
        "include \"test\"\n",
        "|\n",
        "def test_less_than_five(n):\n",
        "  assert_true(n < 5)\n",
        "end\n",
        "| _property_case_result([500], fn(__a): test_less_than_five(__a[0]);)",
    );
    let mut engine = mq_lang::Engine::with_io(
        mq_lang::DefaultModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();

    let result = engine.eval(query, mq_lang::null_input().into_iter()).unwrap();
    let rendered = result.values().first().map(ToString::to_string).unwrap_or_default();

    assert!(
        rendered.contains("Shrunk from [500] to [5]"),
        "unexpected result: {rendered}"
    );
}

#[test]
fn test_module_directories_resolve_includes_outside_the_test_file_directory() {
    let dir = temp_project_dir("module_directories");
    let libs = dir.join("libs");
    let tests = dir.join("tests");
    fs::create_dir_all(&libs).unwrap();
    fs::create_dir_all(&tests).unwrap();
    fs::write(libs.join("lib.mq"), "def add(a, b):\n  a + b\nend\n").unwrap();
    let test_file = tests.join("tests.mq");
    fs::write(
        &test_file,
        "include \"test\" | include \"lib\"\n|\n\ndef test_add():\n  assert_eq(add(1, 2), 3)\nend\n",
    )
    .unwrap();

    let without = TestRunner::new(vec![test_file.clone()]).run().unwrap();
    let with = TestRunner::new(vec![test_file])
        .with_module_directories(vec![libs])
        .run()
        .unwrap();

    assert!(
        !without,
        "module outside the test directory must not resolve by default"
    );
    assert!(with);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_coverage_reports_only_the_imported_module() {
    let dir = temp_project_dir("coverage_only_imported");
    fs::write(
        dir.join("lib.mq"),
        "def add(a, b):\n  a + b\nend\n\ndef unused(a):\n  a * 100\nend\n",
    )
    .unwrap();
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        "include \"test\" | include \"lib\"\n|\n\ndef test_add():\n  assert_eq(add(1, 2), 3)\nend\n",
    )
    .unwrap();
    let output = dir.join("coverage.json");

    let passed = TestRunner::new(vec![test_file])
        .with_coverage(true)
        .with_coverage_format(CoverageFormat::Json)
        .with_coverage_output(Some(output.clone()))
        .run()
        .unwrap();
    assert!(passed);

    let report = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    let files = parsed["files"].as_array().unwrap();

    // Only "lib" is reported: the test file itself and the "test"/"builtin"
    // modules it also pulls in are not the code under test.
    assert_eq!(files.len(), 1, "expected only lib.mq in report: {files:?}");
    assert!(files[0]["file"].as_str().unwrap().ends_with("lib.mq"));
    assert_eq!(files[0]["totalLines"], 2);
    assert_eq!(files[0]["coveredLines"], 1);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_read_file_write_file_use_the_in_memory_mock_not_the_real_disk() {
    let dir = temp_project_dir("mock_io");
    let test_file = dir.join("tests.mq");
    // "/definitely/not/a/real/writable/path.txt" would fail against a real, sandboxed
    // filesystem `Io` (no such directory, no write permission), succeeding here proves
    // `write_file`/`read_file` went through the engine's in-memory mock instead.
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "|\n",
            "def test_write_then_read_file():\n",
            "  write_file(\"/definitely/not/a/real/writable/path.txt\", \"hello-mock\")\n",
            "  | assert_eq(read_file(\"/definitely/not/a/real/writable/path.txt\"), \"hello-mock\")\n",
            "end\n",
        ),
    )
    .unwrap();

    assert!(TestRunner::new(vec![test_file]).run().unwrap());

    assert!(
        !Path::new("/definitely/not/a/real/writable/path.txt").exists(),
        "write_file must not touch the real filesystem"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_mock_fetch_lets_a_test_seed_its_own_http_response() {
    let dir = temp_project_dir("mock_fetch");
    let test_file = dir.join("tests.mq");
    // No real network call is made: `mock_fetch` seeds the in-memory mock's response
    // for the URL, and `http()` reads it back, entirely from within the test function.
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "|\n",
            "def test_reads_mocked_api_response():\n",
            "  mock_fetch(\"https://api.example.com/data\", \"{\\\"ok\\\": true}\")\n",
            "  | assert_eq(http(\"get\", \"https://api.example.com/data\"), \"{\\\"ok\\\": true}\")\n",
            "end\n",
        ),
    )
    .unwrap();

    assert!(TestRunner::new(vec![test_file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_coverage_merges_across_test_files_sharing_a_module() {
    let dir = temp_project_dir("coverage_merge");
    fs::write(
        dir.join("lib.mq"),
        "def add(a, b):\n  a + b\nend\n\ndef sub(a, b):\n  a - b\nend\n",
    )
    .unwrap();
    let test_add = dir.join("test_add.mq");
    fs::write(
        &test_add,
        "include \"test\" | include \"lib\"\n|\n\ndef test_add():\n  assert_eq(add(1, 2), 3)\nend\n",
    )
    .unwrap();
    let test_sub = dir.join("test_sub.mq");
    fs::write(
        &test_sub,
        "include \"test\" | include \"lib\"\n|\n\ndef test_sub():\n  assert_eq(sub(5, 2), 3)\nend\n",
    )
    .unwrap();
    let output = dir.join("coverage.json");

    let passed = TestRunner::new(vec![test_add, test_sub])
        .with_coverage(true)
        .with_coverage_format(CoverageFormat::Json)
        .with_coverage_output(Some(output.clone()))
        .run()
        .unwrap();
    assert!(passed);

    let report = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    let files = parsed["files"].as_array().unwrap();

    assert_eq!(files.len(), 1, "lib.mq must be reported once, merged: {files:?}");
    assert_eq!(files[0]["totalLines"], 2);
    assert_eq!(files[0]["coveredLines"], 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_run_reports_failure_without_aborting_other_files() {
    let dir = temp_project_dir("failure_no_abort");
    let failing = dir.join("failing.mq");
    fs::write(
        &failing,
        "include \"test\"\n|\ndef test_fails():\n  assert_eq(1, 2)\nend\n",
    )
    .unwrap();
    let passing = dir.join("passing.mq");
    fs::write(
        &passing,
        "include \"test\"\n|\ndef test_passes():\n  assert_eq(1, 1)\nend\n",
    )
    .unwrap();

    let passed = TestRunner::new(vec![failing, passing]).run().unwrap();
    assert!(!passed, "run() must report failure when any test fails");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_run_supports_importing_the_test_module() {
    let dir = temp_project_dir("import_test_module");
    let file = dir.join("imported.mq");
    fs::write(
        &file,
        "import \"test\"\n|\ndef test_passes():\n  test::assert_eq(1, 1)\nend\n",
    )
    .unwrap();

    assert!(TestRunner::new(vec![file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_an_error_raised_by_one_test_is_recorded_as_a_failure_and_later_tests_still_run() {
    let mut engine = mq_lang::Engine::with_io(
        mq_lang::DefaultModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();

    let query = r#"include "test" | run_tests_data([
            test_case("boom", fn(): error("boom");),
            test_case("after", fn(): true;)
        ])"#;
    let result = engine
        .eval(query, mq_lang::null_input().into_iter())
        .expect("an erroring test must not abort the evaluation");
    let tests = result.values().first().cloned().unwrap().to_json_value();

    assert_eq!(tests[0]["name"], "boom");
    assert_eq!(tests[0]["status"], "failed");
    assert_eq!(tests[0]["error"], "Error: boom");
    assert_eq!(tests[1]["name"], "after");
    assert_eq!(tests[1]["status"], "passed");
}

#[test]
fn test_format_rollup_lists_failing_and_unrunnable_files_separately() {
    let report = TestRunner::format_rollup(
        &[PathBuf::from("b.mq")],
        &[(PathBuf::from("c.mq"), "rendered".to_string())],
        4,
    );

    assert_eq!(
        report,
        "2 of 4 file(s) did not pass: 1 with failing tests, 1 failed to run\n  \
             ❌ b.mq (tests failed)\n  ⚠ c.mq (failed to run)"
    );
}

#[test]
fn test_run_with_json_format_reports_failure_the_same_way_as_text() {
    let dir = temp_project_dir("json_format_failure");
    let failing = dir.join("failing.mq");
    fs::write(
        &failing,
        "include \"test\"\n|\ndef test_fails():\n  assert_eq(1, 2)\nend\n",
    )
    .unwrap();

    let passed = TestRunner::new(vec![failing])
        .with_format(OutputFormat::Json)
        .run()
        .unwrap();
    assert!(!passed, "run() with --format json must still report failure");

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_run_continues_after_a_file_with_a_syntax_error() {
    let dir = temp_project_dir("continues_after_syntax_error");
    fs::write(dir.join("lib.mq"), "def add(a, b):\n  a + b\nend\n").unwrap();

    let broken = dir.join("a_broken.mq");
    fs::write(
        &broken,
        "include \"test\"\n|\ndef test_broken():\n  assert_eq(1, 1)\nend\nend\n",
    )
    .unwrap();

    let ok = dir.join("b_ok.mq");
    fs::write(
        &ok,
        "include \"test\" | include \"lib\"\n|\n\ndef test_ok():\n  assert_eq(add(1, 2), 3)\nend\n",
    )
    .unwrap();
    let output = dir.join("coverage.json");

    let passed = TestRunner::new(vec![broken, ok])
        .with_coverage(true)
        .with_coverage_format(CoverageFormat::Json)
        .with_coverage_output(Some(output.clone()))
        .run()
        .unwrap();
    assert!(!passed, "a broken file must fail the overall run");

    // Proves b_ok.mq still ran despite a_broken.mq's syntax error.
    let report = fs::read_to_string(&output).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&report).unwrap();
    let files = parsed["files"].as_array().unwrap();
    assert_eq!(
        files.len(),
        1,
        "b_ok.mq must still have run and exercised lib.mq: {files:?}"
    );
    assert!(files[0]["file"].as_str().unwrap().ends_with("lib.mq"));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_render_file_error_attaches_the_file_name_when_mq_lang_left_it_blank() {
    let dir = temp_project_dir("render_file_error");
    let broken = dir.join("broken.mq");
    let content = "include \"test\"\n|\ndef test_x():\n  1\nend\nend\n";
    fs::write(&broken, content).unwrap();

    let tests = TestRunner::discover_tests(content);
    let query = TestRunner::build_test_query(content, &tests);

    let mut engine = mq_lang::Engine::with_io(
        mq_lang::DefaultModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    let err = *engine.eval(&query, mq_lang::null_input().into_iter()).unwrap_err();
    assert_eq!(
        err.source_code.name(),
        "",
        "sanity check: mq-lang itself doesn't know the file name"
    );

    let rendered = TestRunner::render_file_error(&broken, err);
    assert!(
        rendered.contains(&broken.display().to_string()),
        "rendered error must name the failing file:\n{rendered}"
    );

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_filter_skips_non_matching_tests() {
    let dir = temp_project_dir("filter_skips");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "|\n",
            "def test_add():\n  assert_eq(1 + 1, 2)\nend\n\n",
            "def test_sub():\n  assert_eq(2, 3)\nend\n",
        ),
    )
    .unwrap();

    // Only "add" is selected; the failing "sub" test is filtered out.
    let passed = TestRunner::new(vec![test_file])
        .with_filter(Some("add".to_string()))
        .run()
        .unwrap();
    assert!(passed);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_tag_filter_skips_untagged_tests() {
    let dir = temp_project_dir("tag_filter_skips");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        concat!(
            "include \"test\"\n",
            "|\n",
            "# @tags(smoke)\n",
            "def test_add():\n  assert_eq(1 + 1, 2)\nend\n\n",
            "def test_sub():\n  assert_eq(2, 3)\nend\n",
        ),
    )
    .unwrap();

    // Only the "smoke"-tagged test runs; the failing untagged test is filtered out.
    let passed = TestRunner::new(vec![test_file])
        .with_tags(vec!["smoke".to_string()])
        .run()
        .unwrap();
    assert!(passed);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_parallel_execution_runs_all_files() {
    let dir = temp_project_dir("parallel_execution");
    let files: Vec<PathBuf> = (0..5)
        .map(|i| {
            let path = dir.join(format!("test_{i}.mq"));
            fs::write(
                &path,
                format!("include \"test\"\n|\ndef test_case_{i}():\n  assert_eq({i}, {i})\nend\n"),
            )
            .unwrap();
            path
        })
        .collect();

    let passed = TestRunner::new(files).with_parallel_threshold(0).run().unwrap();
    assert!(passed);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_assert_snapshot_creates_then_matches_across_runs() {
    let dir = temp_project_dir("snapshot_create_then_match");
    let test_file = write_greeting_snapshot_test_file(&dir);

    // First run with --update-snapshots creates the golden file and passes.
    assert!(
        TestRunner::new(vec![test_file.clone()])
            .with_update_snapshots(true)
            .run()
            .unwrap()
    );
    assert!(dir.join("__snapshots__/tests/greeting.snap").exists());

    // A normal run now compares against it and still passes.
    assert!(TestRunner::new(vec![test_file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_assert_snapshot_fails_on_mismatch_and_writes_store() {
    let dir = temp_project_dir("snapshot_mismatch");
    let test_file = write_greeting_snapshot_test_file(&dir);
    fs::create_dir_all(dir.join("__snapshots__/tests")).unwrap();
    fs::write(dir.join("__snapshots__/tests/greeting.snap"), "goodbye world").unwrap();

    let passed = TestRunner::new(vec![test_file]).run().unwrap();
    assert!(!passed, "a snapshot mismatch must fail the run");
    assert!(dir.join(".mq-test-store/tests/greeting.diff.html").exists());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_assert_snapshot_missing_golden_fails_without_update_flag() {
    let dir = temp_project_dir("snapshot_missing");
    let test_file = write_greeting_snapshot_test_file(&dir);

    let passed = TestRunner::new(vec![test_file]).run().unwrap();
    assert!(!passed, "a missing golden snapshot must fail rather than silently pass");
    assert!(!dir.join("__snapshots__/tests/greeting.snap").exists());

    fs::remove_dir_all(&dir).ok();
}
