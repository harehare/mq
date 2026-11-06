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
    // Calls `_property_case_result` (from `test_runner.mq`) directly with args `[500]`, bypassing
    // `gen::tuple` entirely so the starting point is deterministic: the smallest `n` for
    // which `n < 5` is false is `5`, so a working shrink search must report exactly that.
    let query = concat!(
        "include \"test\"\n",
        "| include \"test_runner\"\n",
        "|\n",
        "def test_less_than_five(n):\n",
        "  assert_true(n < 5)\n",
        "end\n",
        "| _property_case_result([500], fn(__a): test_less_than_five(__a[0]);)",
    );
    let mut engine = mq_lang::Engine::with_io(
        crate::resolver::TestModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    TestRunner::register_clock(&mut engine);
    super::contract::ContractStats::default().register(&mut engine);

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
        crate::resolver::TestModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    TestRunner::register_clock(&mut engine);
    super::contract::ContractStats::default().register(&mut engine);

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
        crate::resolver::TestModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    TestRunner::register_clock(&mut engine);
    super::contract::ContractStats::default().register(&mut engine);
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

#[rstest]
#[case("@requires(n >= 0)", Some(TestAnnotation::Requires("n >= 0".to_string())))]
#[case("  @ensures(  result > n  )  ", Some(TestAnnotation::Ensures("result > n".to_string())))]
#[case(
        "@ensures(all(headings(doc), fn(h): h.depth <= 5;))",
        Some(TestAnnotation::Ensures("all(headings(doc), fn(h): h.depth <= 5;)".to_string()))
    )]
#[case("@requires", None)]
fn test_parse_annotation_contracts(#[case] comment: &str, #[case] expected: Option<TestAnnotation>) {
    assert_eq!(TestRunner::parse_annotation(comment), expected);
}

#[test]
fn test_contract_annotations_do_not_make_a_function_a_test() {
    let content = "# @requires(n > 0)\n# @ensures(result > 0)\ndef double(n): n * 2;\n";

    assert!(TestRunner::discover_tests(content).is_empty());
}

#[test]
fn test_contract_annotations_do_not_hide_the_test_annotation() {
    let content = "# @requires(n > 0)\n# @property([gen::int(1, 5)])\n# @ensures(result > 0)\ndef check(n): n;\n";

    let tests = TestRunner::discover_tests(content);

    assert!(matches!(tests.as_slice(), [DiscoveredTest::Property { name, .. }] if name == "check"));
}

#[test]
fn test_collect_contracts_gathers_multiple_and_multiline_annotations_with_lines() {
    let content = concat!(
        "# @requires(n >= 0)\n",
        "# @ensures(\n",
        "#   result >= n\n",
        "# )\n",
        "# @ensures(result >= 1)\n",
        "def f(n): n;\n",
    );
    let node = first_def(content);

    let (requires, ensures) = TestRunner::collect_contracts(&node.leading_trivia);

    assert_eq!(
        requires.iter().map(|c| (c.expr.as_str(), c.line)).collect::<Vec<_>>(),
        vec![("n >= 0", 1)]
    );
    assert_eq!(
        ensures
            .iter()
            .map(|c| (c.expr.replace(['\n', '#'], "").trim().to_string(), c.line))
            .collect::<Vec<_>>(),
        vec![("result >= n".to_string(), 2), ("result >= 1".to_string(), 5)]
    );
}

#[test]
fn test_apply_contracts_leaves_content_without_contracts_unchanged() {
    let content = "# @test\ndef f(n): n;\n";

    assert_eq!(TestRunner::apply_contracts(content, false, None).unwrap(), content);
}

#[test]
fn test_apply_contracts_renames_in_place_and_appends_a_wrapper() {
    let content = "# @requires(n > 0)\n# @ensures(result > n)\ndef inc(n): n + 1;\ndef other(): inc(1);\n";

    let rewritten = TestRunner::apply_contracts(content, false, None).unwrap();

    // The original lines keep their numbers: only the `def` name changed.
    assert!(
        rewritten.starts_with(
            "# @requires(n > 0)\n# @ensures(result > n)\ndef __orig_inc(n): n + 1;\ndef other(): inc(1);\n"
        )
    );
    assert!(rewritten.contains("def inc(n) do"), "missing wrapper: {rewritten}");
    assert!(rewritten.contains("\"n > 0\", (n > 0)))"), "{rewritten}");
    assert!(rewritten.contains("let result = __orig_inc(n)"));
    assert!(rewritten.contains("\"result > n\", (result > n)))"), "{rewritten}");
}

#[test]
fn test_apply_contracts_avoids_existing_orig_names() {
    let content = "# @ensures(result == 1)\ndef f(): 1;\ndef __orig_f(): 2;\ndef test_f(): f();\n";

    let rewritten = TestRunner::apply_contracts(content, false).unwrap();

    assert!(rewritten.contains("def __orig_f_(): 1;"), "{rewritten}");
    assert!(rewritten.contains("let result = __orig_f_()"), "{rewritten}");
    assert!(rewritten.contains("def __orig_f(): 2;"), "{rewritten}");
}

#[test]
fn test_apply_contracts_gives_each_original_a_distinct_name() {
    let content = "# @ensures(true)\ndef f(): 1;\n# @ensures(true)\ndef f_(): 2;\n";

    let rewritten = TestRunner::apply_contracts(content, false).unwrap();

    assert!(rewritten.contains("def __orig_f(): 1;"), "{rewritten}");
    assert!(rewritten.contains("def __orig_f_(): 2;"), "{rewritten}");
}

#[test]
fn test_contract_wrapper_calls_its_own_original_when_orig_name_exists() {
    let content = "# @ensures(result == 1)\ndef f(): 1;\ndef __orig_f(): 2;\n";
    let rewritten = TestRunner::apply_contracts(content, false).unwrap();
    assert_eq!(
        run_failures(&format!(
            "include \"test\"\n|\n{rewritten}def test_f(): assert_eq(f(), 1);\n"
        )),
        vec![]
    );
}

#[test]
fn test_apply_contracts_escapes_quotes_in_the_reported_expression() {
    let content = "# @requires(n != \"a\\\"b\")\ndef f(n): n;\n";

    let rewritten = TestRunner::apply_contracts(content, false, None).unwrap();

    assert!(
        rewritten.contains("n != \\\"a\\\\\\\"b\\\""),
        "bad escaping: {rewritten}"
    );
}

#[rstest]
#[case("# @requires(true)\ndef f(*args): args;\n", "variadic parameters are not supported")]
#[case("# @ensures(true)\ndef f(result): result;\n", "clashes")]
fn test_apply_contracts_rejects_unsupported_signatures(#[case] content: &str, #[case] expected: &str) {
    let err = TestRunner::apply_contracts(content, false, None).unwrap_err();

    assert!(err.contains(expected), "unexpected message: {err}");
}

/// Runs `content` like `mq-test --format json` and returns every test's result dict as JSON.
fn run_results(content: &str) -> Vec<serde_json::Value> {
    let tests = TestRunner::discover_tests(content);
    let uses_small_scope = tests.iter().any(|t| matches!(t, DiscoveredTest::SmallScope { .. }));
    let content = TestRunner::apply_contracts(content, uses_small_scope, None).unwrap();
    let query = TestRunner::build_test_query_with_fn(&content, &tests, "run_tests_data");
    let mut engine = mq_lang::Engine::with_io(
        crate::resolver::TestModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    TestRunner::register_clock(&mut engine);
    super::contract::ContractStats::default().register(&mut engine);
    let result = engine.eval(&query, mq_lang::null_input().into_iter()).unwrap();
    let json = result.values().first().cloned().unwrap().to_json_value();
    json.as_array().unwrap().clone()
}

/// Returns `name -> error message` for every failed test (passed tests are absent).
fn run_failures(content: &str) -> Vec<(String, String)> {
    run_results(content)
        .iter()
        .filter(|t| t["status"] == "failed")
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_string(),
                t["error"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// Returns the `detail` report of the passed test `name`.
fn run_detail(content: &str, name: &str) -> String {
    run_results(content)
        .iter()
        .find(|t| t["name"] == name)
        .and_then(|t| t["detail"].as_str().map(str::to_string))
        .unwrap_or_default()
}

#[test]
fn test_contracts_pass_when_satisfied() {
    let content = concat!(
        "# @requires(n >= 0)\n",
        "# @ensures(result > n)\n",
        "def inc(n): n + 1;\n",
        "def test_inc(): assert_eq(inc(2), 3);\n",
    );

    assert_eq!(run_failures(content), vec![]);
}

#[test]
fn test_requires_violation_is_reported_as_the_callers_fault_with_arguments() {
    let content = concat!(
        "# @requires(n >= 0)\n",
        "def inc(n): n + 1;\n",
        "def test_inc(): inc(-4);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 1);
    let message = &failures[0].1;
    assert!(
        message.contains("@requires violated (caller's responsibility)"),
        "{message}"
    );
    assert!(message.contains("inc: n >= 0 (line 1)"), "{message}");
    assert!(message.contains("arguments (n) = [-4]"), "{message}");
}

#[test]
fn test_ensures_violation_is_reported_as_the_functions_fault_with_result() {
    let content = concat!(
        "# @ensures(result > n)\n",
        "def broken(n): n;\n",
        "def test_broken(): broken(3);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 1);
    let message = &failures[0].1;
    assert!(
        message.contains("@ensures violated (function's responsibility)"),
        "{message}"
    );
    assert!(message.contains("broken: result > n (line 1)"), "{message}");
    assert!(message.contains("arguments (n) = [3]"), "{message}");
    assert!(message.contains("result = 3"), "{message}");
}

#[test]
fn test_every_contract_must_hold() {
    let content = concat!(
        "# @requires(a > 0)\n",
        "# @requires(\n",
        "#   b > 0\n",
        "# )\n",
        "def add(a, b): a + b;\n",
        "def test_first(): add(0, 1);\n",
        "def test_second(): add(1, 0);\n",
        "def test_both_ok(): add(1, 1);\n",
    );

    let names: Vec<String> = run_failures(content).into_iter().map(|(name, _)| name).collect();

    assert_eq!(names, vec!["first".to_string(), "second".to_string()]);
}

#[test]
fn test_recursive_calls_go_through_the_contract() {
    // `fact(1)` returns 0 inside the recursion, which only the inner call's `@ensures` can see.
    let content = concat!(
        "# @ensures(result >= 1)\n",
        "def fact(n): if (n <= 0): 1 elif (n == 1): 0 else: n * fact(n - 1);\n",
        "def test_fact(): fact(4);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 1);
    assert!(failures[0].1.contains("arguments (n) = [1]"), "{}", failures[0].1);
}

#[test]
fn test_a_contract_violation_in_a_property_test_is_shrunk() {
    let content = concat!(
        "include \"test\"\n",
        "| import \"gen\"\n",
        "|\n",
        "# @requires(n >= 0)\n",
        "# @ensures(result > n)\n",
        "def bad_inc(n): n;\n",
        "# @property(10, [gen::int(5, 50)])\n",
        "def prop_bad_inc(n): bad_inc(n);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 10, "every case must fail: {failures:?}");
    assert!(
        failures
            .iter()
            .all(|(_, m)| m.contains("@ensures violated") && m.contains("Shrunk from")),
        "{failures:?}"
    );
    assert!(failures.iter().all(|(_, m)| m.contains("to [0]")), "{failures:?}");
}

#[test]
fn test_shrinking_skips_candidates_that_violate_a_requires() {
    // `n` shrinks towards 0 but `@requires(n >= 3)` rules those candidates out, so the
    // minimal counterexample is the smallest valid one.
    let content = concat!(
        "include \"test\"\n",
        "| import \"gen\"\n",
        "|\n",
        "# @requires(n >= 3)\n",
        "# @ensures(result > n)\n",
        "def bad_inc(n): n;\n",
        "# @property(5, [gen::int(20, 50)])\n",
        "def prop_bad_inc(n): bad_inc(n);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 5, "{failures:?}");
    assert!(failures.iter().all(|(_, m)| m.contains("to [3]")), "{failures:?}");
}

#[test]
fn test_run_reports_a_contract_violation_as_a_failed_run() {
    let dir = temp_project_dir("contract_violation");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        "# @ensures(result > n)\ndef broken(n): n;\ndef test_broken(): broken(1);\n",
    )
    .unwrap();

    assert!(!TestRunner::new(vec![test_file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_run_fails_the_file_when_a_contracted_def_is_unsupported() {
    let dir = temp_project_dir("contract_unsupported");
    let test_file = dir.join("tests.mq");
    fs::write(&test_file, "# @requires(true)\ndef f(*xs): xs;\ndef test_f(): f(1);\n").unwrap();

    assert!(!TestRunner::new(vec![test_file]).run().unwrap());

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_markdown_contract_with_a_generated_document_passes_and_shrinks_when_broken() {
    let prelude = concat!("include \"test\"\n", "| import \"gen\"\n", "|\n");
    let good = concat!(
        "# @requires(all(doc, fn(n): not(is_h(n)) || n.depth <= 5;))\n",
        "# @ensures(len(result) == len(doc))\n",
        "def demote_headings(doc):\n",
        "  map(doc, fn(n): if (is_h(n)): to_h(to_text(n), n.depth + 1) else: n;)\n",
        "end\n",
        "# @property(30, [gen::markdown(1, 6, 5)])\n",
        "def prop_demote(src): demote_headings(to_markdown(src));\n",
    );
    assert_eq!(run_failures(&format!("{prelude}{good}")), vec![]);

    // Dropping the last node breaks `@ensures`; the document shrinks to a short one.
    let broken = good.replace(
        "map(doc, fn(n): if (is_h(n)): to_h(to_text(n), n.depth + 1) else: n;)",
        "slice(doc, 0, len(doc) - 1)",
    );
    let failures = run_failures(&format!("{prelude}{broken}"));
    assert!(!failures.is_empty());
    assert!(
        failures
            .iter()
            .all(|(_, m)| m.contains("@ensures violated") && m.contains("Shrunk from")),
        "{failures:?}"
    );
}

#[rstest]
#[case("@small_scope([enumerate::element([1])])", "3", "[enumerate::element([1])]", "{}")]
#[case("@small_scope(5, [enumerate::element([1])])", "5", "[enumerate::element([1])]", "{}")]
#[case(
    "@small_scope(2, [enumerate::element([1, 2])], {\"max_cases\": 10, \"skip_warn\": 0.2})",
    "2",
    "[enumerate::element([1, 2])]",
    "{\"max_cases\": 10, \"skip_warn\": 0.2}"
)]
fn test_parse_annotation_small_scope(
    #[case] comment: &str,
    #[case] size: &str,
    #[case] enumerators: &str,
    #[case] options: &str,
) {
    assert_eq!(
        TestRunner::parse_annotation(comment),
        Some(TestAnnotation::SmallScope {
            max_size_expr: size.to_string(),
            enumerators_expr: enumerators.to_string(),
            options_expr: options.to_string(),
        })
    );
}

#[test]
fn test_parse_annotation_small_scope_rejects_too_many_arguments() {
    assert_eq!(TestRunner::parse_annotation("@small_scope(1, [], {}, 4)"), None);
}

#[test]
fn test_small_scope_annotation_can_span_multiple_lines() {
    let content = "# @small_scope(2,\n#   [enumerate::element([1, 2])])\ndef test_x(n): None;\n";
    let tests = TestRunner::discover_tests(content);
    assert_eq!(tests.len(), 1);
    assert_eq!(tests[0].kind(), "small_scope");
}

#[test]
fn test_split_top_level_commas_ignores_nested_commas_and_strings() {
    assert_eq!(
        TestRunner::split_top_level_commas("1, [a, b], {\"k\": \"x,y\"}"),
        vec!["1", " [a, b]", " {\"k\": \"x,y\"}"]
    );
}

#[test]
fn test_small_scope_passes_and_reports_its_scope() {
    let content = concat!(
        "# @small_scope(2, [enumerate::array(enumerate::element([1, 2]), 2)])\n",
        "def test_short(xs): assert_true(len(xs) <= 2);\n",
    );

    assert_eq!(run_failures(content), vec![]);
    let detail = run_detail(content, "short");
    assert!(detail.contains("No counterexample found within this scope"), "{detail}");
    assert!(detail.contains("| 2 | 4 | 4 | 0 |"), "{detail}");
    assert!(detail.contains("| **all** | **7** | **7** | **0** |"), "{detail}");
    assert!(detail.contains("size <= 2"), "{detail}");
    assert!(detail.contains("7 valid, 0 skipped"), "{detail}");
    assert!(!detail.contains("guarantee") && !detail.contains("proved"), "{detail}");
}

#[test]
fn test_small_scope_reports_the_smallest_counterexample_first() {
    let content = concat!(
        "# @small_scope(4, [enumerate::array(enumerate::element([1, 2]), 4)])\n",
        "def test_short(xs): assert_true(len(xs) < 2);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 1);
    assert!(
        failures[0]
            .1
            .contains("Counterexample found at size 2: args = [[1, 1]]"),
        "{}",
        failures[0].1
    );
}

#[test]
fn test_small_scope_requires_violation_of_a_direct_call_is_skipped_and_counted() {
    let content = concat!(
        "# @requires(n > 0)\n",
        "def pos(n): n;\n",
        "# @small_scope(0, [enumerate::element([0, 1, 2, 3])])\n",
        "def test_pos(n): assert_true(pos(n) >= 1);\n",
    );

    assert_eq!(run_failures(content), vec![]);
    let detail = run_detail(content, "pos");
    assert!(detail.contains("3 valid, 1 skipped"), "{detail}");
    assert!(!detail.contains("warning"), "{detail}");
}

#[test]
fn test_small_scope_requires_violation_inside_another_contracted_function_fails() {
    let content = concat!(
        "# @requires(n > 0)\n",
        "def pos(n): n;\n",
        "# @requires(n > 0)\n",
        "def outer(n): pos(n - 5);\n",
        "# @small_scope(0, [enumerate::element([1])])\n",
        "def test_outer(n): assert_true(outer(n) >= 1);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].1.contains("@requires violated in nested call"),
        "{}",
        failures[0].1
    );
    assert!(failures[0].1.contains("pos: n > 0"), "{}", failures[0].1);
}

#[test]
fn test_small_scope_ensures_violation_and_other_errors_fail() {
    let content = concat!(
        "# @ensures(result > n)\n",
        "def bad_inc(n): n;\n",
        "# @small_scope(0, [enumerate::element([1])])\n",
        "def test_ensures(n): bad_inc(n);\n",
        "# @small_scope(0, [enumerate::element([1])])\n",
        "def test_error(n): error(\"boom\");\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 2, "{failures:?}");
    assert!(failures[0].1.contains("@ensures violated"), "{}", failures[0].1);
    assert!(failures[1].1.contains("boom"), "{}", failures[1].1);
}

#[test]
fn test_small_scope_with_no_valid_input_fails() {
    let content = concat!(
        "# @requires(n > 0)\n",
        "def pos(n): n;\n",
        "# @small_scope(0, [enumerate::element([0, -1])])\n",
        "def test_pos(n): assert_true(pos(n) >= 1);\n",
    );

    let failures = run_failures(content);

    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].1.contains("No valid input was checked (2 skipped"),
        "{}",
        failures[0].1
    );
}

#[test]
fn test_small_scope_warns_when_most_inputs_are_skipped() {
    let content = concat!(
        "# @requires(n > 0)\n",
        "def pos(n): n;\n",
        "# @small_scope(0, [enumerate::element([0, 0, 0, 1])])\n",
        "def test_pos(n): assert_true(pos(n) >= 1);\n",
        "# @small_scope(0, [enumerate::element([0, 0, 0, 1])], {\"skip_warn\": 0.9})\n",
        "def test_quiet(n): assert_true(pos(n) >= 1);\n",
    );

    assert_eq!(run_failures(content), vec![]);
    let detail = run_detail(content, "pos");
    assert!(detail.contains("warning: 75% of the inputs were skipped"), "{detail}");
    assert!(detail.contains("enumerator options"), "{detail}");
    assert!(!run_detail(content, "quiet").contains("warning"));
}

#[rstest]
#[case("{\"max_cases\": 2}", "max_cases = 2")]
#[case("{\"max_total\": 2}", "max_total = 2")]
fn test_small_scope_stops_with_an_error_when_a_limit_is_exceeded(#[case] options: &str, #[case] expected: &str) {
    let content = format!(
        "# @small_scope(2, [enumerate::array(enumerate::element([1]), 5)], {options})\n\
         def test_any(xs): assert_true(true);\n"
    );

    let failures = run_failures(&content);

    assert_eq!(failures.len(), 1);
    assert!(
        failures[0].1.contains("Enumeration stopped before finishing"),
        "{}",
        failures[0].1
    );
    assert!(failures[0].1.contains(expected), "{}", failures[0].1);
}

#[test]
fn test_small_scope_with_mismatched_enumerator_count_fails_the_run() {
    let dir = temp_project_dir("small_scope_arity_mismatch");
    let test_file = dir.join("tests.mq");
    fs::write(
        &test_file,
        "# @small_scope([enumerate::element([1]), enumerate::element([2])])\ndef test_one(a): true;\n",
    )
    .unwrap();

    assert!(!TestRunner::new(vec![test_file]).run().unwrap_or(false));

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn test_small_scope_markdown_enumerator_builds_documents_that_satisfy_a_precondition() {
    let content = concat!(
        "# @requires(len(to_markdown(src)) > 0)\n",
        "def first_block(src): to_markdown(src)[0];\n",
        "# @small_scope(2, [enumerate::markdown({\"kinds\": [\"heading\", \"paragraph\"], \"min_blocks\": 1})])\n",
        "def test_first_block(src): assert_true(is_markdown(first_block(src)));\n",
    );

    assert_eq!(run_failures(content), vec![]);
    let detail = run_detail(content, "first_block");
    assert!(detail.contains("0 skipped"), "{detail}");
    assert!(detail.contains("kinds: h1-h6, paragraph"), "{detail}");
}

#[rstest]
#[case("{}", 0, 1)]
#[case("{}", 1, 11)]
#[case("{}", 2, 134)]
#[case("{\"kinds\": [\"heading\"], \"max_heading_depth\": 2}", 1, 2)]
#[case("{\"kinds\": [\"heading\"], \"max_heading_depth\": 2}", 2, 4)]
#[case("{\"kinds\": [\"table\"], \"max_table_rows\": 1, \"min_blocks\": 1}", 1, 4)]
#[case(
    "{\"kinds\": [\"table\"], \"min_table_cols\": 2, \"max_table_cols\": 3, \"min_table_rows\": 1, \"max_table_rows\": 2, \"min_blocks\": 1}",
    1,
    4
)]
fn test_enumerate_markdown_tier_sizes(#[case] options: &str, #[case] size: usize, #[case] expected: usize) {
    // Tier 0 is the empty document, tier 1 a single block.
    let content = format!(
        "# @small_scope(0, [enumerate::element([0])])\n\
         def test_count(_n): assert_eq(len(enumerate::tier(enumerate::markdown({options}), {size})), {expected});\n"
    );

    assert_eq!(run_failures(&content), vec![]);
}

#[test]
fn test_apply_contracts_labels_nested_requires_only_when_asked() {
    let content = "# @requires(n > 0)\ndef f(n): n;\n";

    assert!(
        !TestRunner::apply_contracts(content, false, None)
            .unwrap()
            .contains("in nested call")
    );
    assert!(
        TestRunner::apply_contracts(content, true, None)
            .unwrap()
            .contains("in nested call")
    );
}

#[test]
fn test_enumerate_value_enumerators_list_values_by_size() {
    let content = concat!(
        "# @small_scope(0, [enumerate::element([0])])\n",
        "def test_values(_n):\n",
        "  assert_eq(\n",
        "    [\n",
        "      enumerate::tier(enumerate::int(-5, 5), 0),\n",
        "      enumerate::tier(enumerate::int(-5, 5), 2),\n",
        "      enumerate::tier(enumerate::int(0, 5), 2),\n",
        "      enumerate::tier(enumerate::int(1, 5), 0),\n",
        "      enumerate::tier(enumerate::string(\"ab\", 2), 0),\n",
        "      enumerate::tier(enumerate::string(\"ab\", 2), 2),\n",
        "      enumerate::tier(enumerate::string(\"\", 2), 1),\n",
        "      enumerate::tier(enumerate::bool(), 0),\n",
        "      enumerate::tier(enumerate::dict_of(enumerate::element([\"a\", \"b\"]), enumerate::element([1]), 2), 1),\n",
        "      enumerate::tier(enumerate::dict_of(enumerate::element([\"a\"]), enumerate::element([1, 2]), 2), 1),\n",
        "      enumerate::tier(enumerate::dict_of(enumerate::element([\"a\", \"b\"]), enumerate::element([1]), 2), 2),\n",
        "    ],\n",
        "    [\n",
        "      [0],\n",
        "      [2, -2],\n",
        "      [2],\n",
        "      [],\n",
        "      [\"\"],\n",
        "      [\"aa\", \"ab\", \"ba\", \"bb\"],\n",
        "      [],\n",
        "      [true, false],\n",
        "      [{\"a\": 1}, {\"b\": 1}],\n",
        "      [{\"a\": 1}, {\"a\": 2}],\n",
        "      [{\"a\": 1, \"b\": 1}],\n",
        "    ]\n",
        "  )\n",
        "end\n",
    );

    assert_eq!(run_failures(content), vec![]);
}

#[rstest]
#[case("{}", &[7, 25, 159])]
#[case("{\"null\": false}", &[6, 22, 124])]
#[case("{\"max_depth\": 0}", &[5, 4, 4])]
fn test_enumerate_json_tier_sizes(#[case] options: &str, #[case] expected: &[usize]) {
    let content = format!(
        "# @small_scope(0, [enumerate::element([0])])\n\
         def test_sizes(_n):\n  \
           assert_eq(map(range(0, {last}), fn(k): len(enumerate::tier(enumerate::json({options}), k));), {expected:?})\n\
         end\n",
        last = expected.len() - 1
    );

    assert_eq!(run_failures(&content), vec![], "{content}");
}

#[test]
fn test_durations_have_millisecond_resolution() {
    // `now()` only counts whole seconds, so a short test would always report 0.
    let content = "def test_slow(): len(fold(range(0, 30000), 0, fn(a, b): a + b;)) >= 0;\n";

    let duration = run_results(content)[0]["duration"].as_i64().unwrap();

    assert!(duration > 0, "duration was {duration} ms");
}

#[test]
fn test_emit_smt_translates_contracts_and_skips_unsupported_bodies() {
    let content = "\
# @requires(lo <= hi)
# @ensures(result >= lo && result <= hi)
def clamp(x, lo, hi): min(max(x, lo), hi);

# @ensures(result == a)
def pick(a, b): if (a): b else: false;

# @ensures(len(result) > 0)
def unsupported(a): foo(a);
";
    let smt = TestRunner::emit_smt(content).unwrap();
    assert!(smt.contains("(define-fun clamp ((x Int) (lo Int) (hi Int)) Int"));
    assert!(smt.contains("(assert (<= lo hi))"));
    assert!(smt.contains("(let ((result (clamp x lo hi))) (not (and (>= result lo) (<= result hi))))"));
    assert!(smt.contains("(define-fun pick ((a Bool) (b Bool)) Bool (ite a b false))"));
    assert!(smt.contains("; skipped unsupported: unsupported function `foo`"));
}

#[test]
fn test_emit_smt_uses_define_fun_rec_for_recursion() {
    let content = "# @ensures(result >= 0)\ndef f(n): if (n <= 0): 0 else: f(n - 1) + 1;\n";
    assert!(
        TestRunner::emit_smt(content)
            .unwrap()
            .contains("(define-fun-rec f ((n Int)) Int")
    );
}

#[test]
fn test_emit_smt_rejects_ill_sorted_contracts() {
    let content = "# @ensures(result && 1)\ndef m(a, b): a && b;\n";
    assert!(
        TestRunner::emit_smt(content)
            .unwrap()
            .contains("; skipped m: and: expected Bool, found Int")
    );
}

#[test]
fn test_apply_contracts_keeps_default_parameters_in_the_wrapper() {
    let content = "# @requires(n >= 0)\ndef f(a, n = len([1, 2]) + 1, flag = false): a;\n";
    let rewritten = TestRunner::apply_contracts(content, false, None).unwrap();
    assert!(
        rewritten.contains("def f(a, n = len([1, 2]) + 1, flag = false) do"),
        "{rewritten}"
    );
    assert!(rewritten.contains("__orig_f(a, n, flag)"), "{rewritten}");
}

#[test]
fn test_contract_stats_count_evaluations_and_unmet_requires() {
    let content = concat!(
        "# @requires(n >= 0)\n# @ensures(result >= n)\ndef f(n): n;\n",
        "# @ensures(result > 0)\ndef never_called(n): n;\n",
        "def test_calls():\n  f(1) | f(2) | try: f(-1) catch: 0\nend\n",
    );
    let stats = super::contract::ContractStats::default();
    let rewritten = TestRunner::apply_contracts(content, false, Some(&stats)).unwrap();
    let tests = TestRunner::discover_tests(&rewritten);
    let query = TestRunner::build_test_query(&rewritten, &tests);
    let mut engine = mq_lang::Engine::with_io(
        crate::resolver::TestModuleResolver::default(),
        mq_lang::Shared::new(mq_lang::MemIo::default()),
    );
    engine.load_builtin_module();
    TestRunner::register_clock(&mut engine);
    stats.register(&mut engine);
    engine.register_fn("current_test_file", || -> Result<String, mq_lang::HostFunctionError> {
        Ok("t.mq".to_string())
    });
    engine.eval(&query, mq_lang::null_input().into_iter()).unwrap();

    let table = stats.markdown().unwrap();
    assert!(
        table.contains("| ↷ | `f` | `@requires(n >= 0)` (line 1) | 3 | 1 |"),
        "{table}"
    );
    assert!(
        table.contains("| ✅ | `f` | `@ensures(result >= n)` (line 2) | 2 | 0 |"),
        "{table}"
    );
}
