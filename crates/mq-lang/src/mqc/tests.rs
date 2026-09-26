use super::*;
use crate::tarn::bytecode::{Chunk, OpCode, ParamBinding};
use crate::{DefaultEngine, RuntimeValue};
use proptest::prelude::*;
use rstest::rstest;

fn engine() -> DefaultEngine {
    let mut engine = DefaultEngine::default();
    engine.load_builtin_module();
    engine
}

fn compile(code: &str) -> Vec<u8> {
    engine().compile_to_mqc(code, &[]).expect("compile to .mqc")
}

fn run_mqc(bytes: &[u8], input: Vec<RuntimeValue>) -> crate::MqResult {
    let mut engine = engine();
    let program = engine.load_mqc(bytes).expect("load .mqc");
    engine.eval_compiled(program.program(), input.into_iter())
}

fn markdown(text: &str) -> Vec<RuntimeValue> {
    crate::parse_markdown_input(text).unwrap()
}

/// Rewrites a valid file's sections and recomputes its checksum.
fn rewrite(bytes: &[u8], edit: impl FnOnce(&mut Vec<Section<'static>>)) -> Vec<u8> {
    let mut sections: Vec<Section<'static>> = read_container(bytes)
        .unwrap()
        .into_iter()
        .map(|section| Section {
            payload: Cow::Owned(section.payload.into_owned()),
            ..section
        })
        .collect();
    edit(&mut sections);
    write_container(&sections).unwrap()
}

fn replace_meta(bytes: &[u8], edit: impl FnOnce(&mut Meta)) -> Vec<u8> {
    rewrite(bytes, |sections| {
        let section = sections.iter_mut().find(|section| section.tag == META).unwrap();
        let mut meta = decode_meta(&section.payload).unwrap();
        edit(&mut meta);
        section.payload = Cow::Owned(encode_meta(&meta).unwrap());
    })
}

#[rstest]
#[case::selector(".h2", "# a\n\n## b\n\n## c\n")]
#[case::builtin_prelude("select(.h) | to_text() | upcase()", "# title\n\ntext\n")]
#[case::user_function("def twice(x): x + x; | .text | twice(to_text())", "hello\n")]
#[case::closure("let add = fn(a): fn(b): a + b;; | let inc = add(1) | inc(2)", "x\n")]
#[case::recursion("def fact(n): if (n <= 1): 1 else: n * fact(n - 1); | fact(10)", "x\n")]
#[case::loops("var s = 0 | foreach(x, range(1, 10)): s += x; | s", "x\n")]
#[case::dict_literal(r#"{"a": 1, "b": [1, 2, {"c": true}]}"#, "x\n")]
#[case::string_interpolation(r#"let n = "mq" | s"hello ${n}""#, "x\n")]
#[case::try_catch(r#"try: error("boom") catch(e): s"caught ${e}""#, "x\n")]
#[case::generator("def gen(): yield 1 | yield 2; | let g = gen() | [next(g), next(g)]", "x\n")]
#[case::optional_params(r#"def greet(name, greeting = "hi"): s"${greeting} ${name}"; | greet("mq")"#, "x\n")]
#[case::nodes(r#"let title = to_text(.h1) | nodes | len()"#, "# a\n\ntext\n\n- list\n")]
#[case::nodes_with_let(r#".h | let last = to_text() | nodes | last"#, "# a\n\n## b\n")]
#[case::standard_module(r#"import "csv" | csv::csv_parse("a,b\n1,2", true)"#, "x\n")]
#[case::inline_module(r#"module m: let base = 40 | def f(x): x + base; end | m::f(2)"#, "x\n")]
#[case::attr_selector(".link.url", "[mq](https://mqlang.org)\n")]
#[case::table_selector(".[1][0]", "| a | b |\n|---|---|\n| 1 | 2 |\n")]
#[case::list_selector(".[1]", "- one\n- two\n")]
fn test_mqc_round_trip_matches_eval(#[case] query: &str, #[case] input: &str) {
    let expected = engine().eval(query, markdown(input).into_iter()).unwrap();
    let actual = run_mqc(&compile(query), markdown(input)).unwrap();
    assert_eq!(actual, expected, "query: {query}");
}

#[test]
fn test_mqc_program_reads_engine_globals_at_run_time() {
    let bytes = compile("greeting + \" mq\"");
    let mut engine = engine();
    engine.define_string_value("greeting", "hello");
    let program = engine.load_mqc(&bytes).unwrap();
    assert_eq!(program.external_globals(), ["greeting".to_string()]);
    let result = engine
        .eval_compiled(program.program(), crate::null_input().into_iter())
        .unwrap();
    assert_eq!(result, vec!["hello mq".to_string().into()].into());
}

#[test]
fn test_mqc_program_sees_globals_changed_between_runs() {
    let bytes = compile("greeting");
    let mut engine = engine();
    let program = engine.load_mqc(&bytes).unwrap();
    for greeting in ["hello", "bye"] {
        engine.define_string_value("greeting", greeting);
        let result = engine
            .eval_compiled(program.program(), crate::null_input().into_iter())
            .unwrap();
        assert_eq!(result, vec![greeting.to_string().into()].into());
    }
}

#[test]
fn test_mqc_program_is_reusable_across_inputs() {
    let bytes = compile("upcase()");
    let mut engine = engine();
    let program = engine.load_mqc(&bytes).unwrap();
    for word in ["a", "b"] {
        let result = engine
            .eval_compiled(program.program(), crate::raw_input(word).into_iter())
            .unwrap();
        assert_eq!(result, vec![word.to_uppercase().into()].into());
    }
}

#[test]
fn test_mqc_metadata_and_dependencies_round_trip() {
    let bytes = engine()
        .compile_to_mqc(r#"import "csv" | csv::csv_parse(true)"#, &[("input-format", "csv")])
        .unwrap();
    let program = engine().load_mqc(&bytes).unwrap();
    assert_eq!(program.metadata("input-format"), Some("csv"));
    assert_eq!(program.metadata("missing"), None);
    let csv = program
        .dependencies()
        .iter()
        .find(|dependency| dependency.name == "csv")
        .expect("csv dependency");
    assert_eq!(csv.specifier, "csv");
    assert_eq!(csv.sha256.len(), 64);
}

#[test]
fn test_mqc_runtime_error_points_at_original_source() {
    let query = "def f(x):\n  x / 0;\n| f(1)";
    let error = run_mqc(&compile(query), crate::null_input()).unwrap_err();
    assert_eq!(error.source_code.inner(), query);
    let offset = error.location.offset();
    assert_eq!(&query[offset..offset + 1], "/", "location: {offset}");
}

#[rstest]
#[case::call_site(r#"import "csv" | csv::csv_parse(1, 2, 3)"#)]
#[case::inside_imported_module(r#"import "csv" | csv::csv_parse(true)"#)]
#[case::inside_included_module(r#"include "csv" | csv_parse(true)"#)]
fn test_mqc_standard_module_error_matches_eval(#[case] query: &str) {
    let expected = engine().eval(query, markdown("# a\n").into_iter()).unwrap_err();
    let actual = run_mqc(&compile(query), markdown("# a\n")).unwrap_err();
    assert_eq!(actual.cause.to_string(), expected.cause.to_string());
    assert_eq!(actual.source_code.name(), expected.source_code.name());
    assert_eq!(actual.source_code.inner(), expected.source_code.inner());
    assert_eq!(actual.location, expected.location);
}

fn source_files(bytes: &[u8]) -> Vec<SourceFile> {
    let sections = read_container(bytes).unwrap();
    let section = sections.iter().find(|section| section.tag == SOURCE).unwrap();
    decode_source(&section.payload).unwrap().0
}

#[test]
fn test_mqc_omits_every_standard_module_source() {
    for (name, source) in crate::STANDARD_MODULES.iter() {
        let functions: Vec<&str> = source()
            .lines()
            .filter_map(|line| line.strip_prefix("def ")?.split('(').next())
            .collect();
        let query = format!(r#"include "{name}" | len([{}])"#, functions.join(", "));
        let bytes = compile(&query);
        let files = source_files(&bytes);
        assert!(
            files.iter().any(|file| file.name == name.as_str()),
            "{name} not referenced"
        );
        for file in &files {
            let expected = (file.name == crate::Module::TOP_LEVEL_MODULE).then(|| query.clone());
            assert_eq!(file.text, expected, "{name}: source of {}", file.name);
        }
        let cause = |result: crate::MqResult| result.map_err(|error| error.cause.to_string());
        let expected = cause(engine().eval(&query, markdown("# a\n").into_iter()));
        assert_eq!(cause(run_mqc(&bytes, markdown("# a\n"))), expected, "{name}");
    }
}

#[rstest]
#[case::inline_module(r#"module m: def f(): "inline"; end | m::f()"#, "inline")]
#[case::shadowed_standard_module(r#"import "csv" | csv::label()"#, "shadow")]
fn test_mqc_keeps_source_of_modules_not_bundled(#[case] query: &str, #[case] output: &str) {
    const SHADOW_CSV: &str = r#"def label(): "shadow";"#;

    #[derive(Clone, Default)]
    struct ShadowingResolver;

    impl crate::ModuleResolver for ShadowingResolver {
        fn resolve(&self, name: &str) -> Result<String, crate::ModuleError> {
            match name {
                "csv" => Ok(SHADOW_CSV.to_string()),
                _ => Err(crate::ModuleError::NotFound(format!("{name}.mq").into())),
            }
        }
        fn get_path(&self, name: &str) -> Result<String, crate::ModuleError> {
            Ok(name.to_string())
        }
        fn search_paths(&self) -> Vec<std::path::PathBuf> {
            Vec::new()
        }
        fn set_search_paths(&mut self, _paths: Vec<std::path::PathBuf>) {}
    }

    let mut shadowing = Engine::new(ShadowingResolver);
    shadowing.load_builtin_module();
    let bytes = shadowing.compile_to_mqc(query, &[]).unwrap();
    for file in source_files(&bytes) {
        if file.name != crate::Module::BUILTIN_MODULE {
            assert!(file.text.is_some(), "source of {} omitted", file.name);
        }
    }
    assert!(
        source_files(&bytes)
            .iter()
            .all(|file| file.text.as_deref() != Some(standard_module_source("csv").unwrap()))
    );
    // Runs without the resolver that produced the module.
    let result = run_mqc(&bytes, crate::null_input()).unwrap();
    assert_eq!(result, vec![output.to_string().into()].into());
}

#[test]
fn test_load_mqc_rejects_missing_source_of_unbundled_module() {
    let bytes = rewrite(&compile("upcase()"), |sections| {
        let section = sections.iter_mut().find(|section| section.tag == SOURCE).unwrap();
        let (mut files, spans) = decode_source(&section.payload).unwrap();
        for file in &mut files {
            file.text = None;
        }
        section.payload = Cow::Owned(encode_source(&files, &spans).unwrap());
    });
    let error = engine().load_mqc(&bytes).unwrap_err();
    assert!(
        matches!(&error, MqcError::Malformed(message) if message.contains(crate::Module::TOP_LEVEL_MODULE)),
        "{error:?}"
    );
}

#[rstest]
#[case::bare_env("$MQC_TEST_ENV")]
#[case::module_let(r#"module m: let v = s"${$MQC_TEST_ENV}" end | m::v"#)]
#[case::caught_in_module_let(r#"module m: let v = try: s"${$MQC_TEST_ENV}" catch: "fallback" end | m::v"#)]
fn test_compile_to_mqc_rejects_compile_time_env_reads(#[case] query: &str) {
    let error = engine().compile_to_mqc(query, &[]).unwrap_err();
    assert!(
        matches!(&error, MqcError::EnvironmentAtCompileTime(name) if name == "MQC_TEST_ENV"),
        "{error:?}"
    );
}

#[rstest]
#[case::plain("arg", true)]
#[case::binary_op(r#"arg + "!""#, true)]
#[case::interpolation(r#"s"${arg}""#, false)]
#[case::array("[1, arg]", true)]
#[case::dict(r#"{"k": arg}"#, true)]
#[case::condition("if (arg): 1 else: 2", true)]
#[case::call_argument("upcase(arg)", true)]
fn test_compile_to_mqc_rejects_runtime_names_in_module_let(#[case] initializer: &str, #[case] has_location: bool) {
    let query = format!("module m: let v = {initializer} end | m::v");
    let error = engine().compile_to_mqc(&query, &[]).unwrap_err();
    let MqcError::ModuleLevelNotDefined { name, source } = &error else {
        panic!("{error:?}");
    };
    assert_eq!(name, "arg");
    // Names inside string interpolation carry no source position.
    if has_location {
        let offset = source.location.offset();
        assert_eq!(&query[offset..offset + 3], "arg", "location: {offset}");
    }
}

#[rstest]
#[case::top_level_let("let v = arg | v")]
#[case::interpolation(r#"s"${arg}!""#)]
#[case::module_function("module m: def f(): arg; end | m::f()")]
#[case::module_closure("module m: let f = fn(): arg; end | m::f()")]
#[case::module_constant_and_function(r#"module m: let suffix = "!" | def f(): arg + suffix; end | m::f()"#)]
fn test_mqc_defers_runtime_names_outside_module_let(#[case] query: &str) {
    let with_arg = || {
        let engine = engine();
        engine.define_string_value("arg", "hello");
        engine
    };
    let expected = with_arg().eval(query, crate::null_input().into_iter()).unwrap();
    let mut engine = with_arg();
    let program = engine.load_mqc(&compile(query)).unwrap();
    let actual = engine
        .eval_compiled(program.program(), crate::null_input().into_iter())
        .unwrap();
    assert_eq!(actual, expected, "query: {query}");
}

#[cfg(feature = "debug-trace")]
#[rstest]
#[case::single_phase("upcase()", &["phase: main", "CallBuiltin upcase"])]
#[case::nodes_split(".h | nodes | len()", &["phase: per-input", "phase: nodes aggregate"])]
#[case::user_function("def f(x): x + 1; | f(1)", &["chunks: 2"])]
fn test_dump_bytecode_renders_loaded_program(#[case] query: &str, #[case] expected: &[&str]) {
    let mut engine = engine();
    let program = engine.load_mqc(&compile(query)).unwrap();
    let dump = engine.dump_bytecode(program.program()).unwrap();
    for text in expected {
        assert!(dump.contains(text), "missing {text:?} in:\n{dump}");
    }
}

#[cfg(feature = "debug-trace")]
#[rstest]
#[case::builtin_call("upcase()")]
#[case::breakpoint("breakpoint() | upcase()")]
#[case::user_function("def f(x): x + 1; | f(1)")]
fn test_dump_bytecode_of_loaded_program_is_uninstrumented(#[case] query: &str) {
    let mut engine = engine();
    let program = engine.load_mqc(&compile(query)).unwrap();
    let dump = engine.dump_bytecode(program.program()).unwrap();
    for opcode in ["StmtBoundary", "SyncCallNode", "Breakpoint"] {
        assert!(!dump.contains(opcode), "unexpected {opcode} in:\n{dump}");
    }
}

#[test]
fn test_mqc_reads_interpolated_env_at_run_time() {
    let bytes = compile(r#"s"${$MQC_RUNTIME_ENV}""#);
    let io = crate::io::MemIo::default().with_env("MQC_RUNTIME_ENV", "run");
    let mut engine = Engine::with_default_io(Shared::new(crate::SandboxedIo::new(io).allow_env(true)));
    engine.load_builtin_module();
    let program = engine.load_mqc(&bytes).unwrap();
    let result = engine
        .eval_compiled(program.program(), crate::null_input().into_iter())
        .unwrap();
    assert_eq!(result, vec!["run".to_string().into()].into());
}

#[test]
fn test_compile_to_mqc_reports_syntax_errors() {
    let error = engine().compile_to_mqc("def f(:", &[]).unwrap_err();
    assert!(matches!(error, MqcError::Compile(_)), "{error:?}");
}

#[test]
fn test_mqc_round_trips_function_valued_module_let() {
    let query = "module m: let f = fn(x): x + 1; end | m::f(1)";
    let result = run_mqc(&compile(query), crate::null_input()).unwrap();
    assert_eq!(result, vec![RuntimeValue::Number(2.into())].into());
}

#[rstest]
#[case::empty(&[])]
#[case::not_mqc(b"select(.h1)")]
#[case::magic_only(b"MQC\0")]
fn test_load_mqc_rejects_non_mqc_input(#[case] bytes: &[u8]) {
    assert!(matches!(
        engine().load_mqc(bytes),
        Err(MqcError::NotMqc | MqcError::Malformed(_))
    ));
}

#[test]
fn test_load_mqc_detects_corruption() {
    let mut bytes = compile(".h1");
    let middle = bytes.len() / 2;
    bytes[middle] ^= 0xFF;
    assert!(matches!(engine().load_mqc(&bytes), Err(MqcError::ChecksumMismatch)));
}

#[test]
fn test_load_mqc_detects_truncation() {
    let bytes = compile(".h1");
    assert!(matches!(
        engine().load_mqc(&bytes[..bytes.len() - 1]),
        Err(MqcError::Malformed(_))
    ));
}

#[test]
fn test_load_mqc_rejects_unknown_container_version() {
    let mut bytes = compile(".h1");
    bytes[4..6].copy_from_slice(&2u16.to_le_bytes());
    let len = bytes.len() - CHECKSUM_LEN;
    let checksum = Sha256::digest(&bytes[..len]);
    bytes[len..].copy_from_slice(&checksum);
    assert!(matches!(
        engine().load_mqc(&bytes),
        Err(MqcError::UnsupportedContainerVersion(2))
    ));
}

#[test]
fn test_load_mqc_skips_unknown_optional_section() {
    let bytes = rewrite(&compile("upcase()"), |sections| {
        sections.push(Section {
            tag: *b"XTRA",
            version: 7,
            required: false,
            payload: Cow::Owned(vec![1, 2, 3]),
        });
    });
    let result = run_mqc(&bytes, crate::raw_input("mq")).unwrap();
    assert_eq!(result, vec!["MQ".to_string().into()].into());
}

#[test]
fn test_load_mqc_rejects_unknown_required_section() {
    let bytes = rewrite(&compile(".h1"), |sections| {
        sections.push(Section {
            tag: *b"XTRA",
            version: 1,
            required: true,
            payload: Cow::Owned(Vec::new()),
        });
    });
    assert!(matches!(
        engine().load_mqc(&bytes),
        Err(MqcError::UnknownRequiredSection(tag)) if tag == "XTRA"
    ));
}

#[test]
fn test_load_mqc_rejects_duplicate_and_missing_sections() {
    let duplicated = rewrite(&compile(".h1"), |sections| {
        let meta = sections.iter().find(|section| section.tag == META).unwrap();
        sections.push(Section {
            payload: meta.payload.clone(),
            ..*meta
        });
    });
    assert!(matches!(
        engine().load_mqc(&duplicated),
        Err(MqcError::DuplicateSection(_))
    ));

    let missing = rewrite(&compile(".h1"), |sections| {
        sections.retain(|section| section.tag != DEPS)
    });
    assert!(matches!(
        engine().load_mqc(&missing),
        Err(MqcError::MissingSection("DEPS"))
    ));
}

#[test]
fn test_load_mqc_rejects_unknown_section_version() {
    let bytes = rewrite(&compile(".h1"), |sections| {
        sections.iter_mut().find(|section| section.tag == CODE).unwrap().version = 2;
    });
    assert!(matches!(
        engine().load_mqc(&bytes),
        Err(MqcError::UnsupportedSectionVersion { version: 2, .. })
    ));
}

#[rstest]
#[case::abi(|meta: &mut Meta| meta.vm_abi += 1)]
#[case::version(|meta: &mut Meta| meta.mq_version = "0.0.0".to_string())]
fn test_load_mqc_rejects_incompatible_vm(#[case] edit: fn(&mut Meta)) {
    let bytes = replace_meta(&compile(".h1"), edit);
    assert!(matches!(
        engine().load_mqc(&bytes),
        Err(MqcError::IncompatibleVm { .. })
    ));
}

#[test]
fn test_load_mqc_rejects_missing_builtins() {
    let bytes = replace_meta(&compile(".h1"), |meta| {
        meta.required_builtins.push("no_such_builtin".to_string())
    });
    assert!(matches!(
        engine().load_mqc(&bytes),
        Err(MqcError::MissingBuiltins(names)) if names == ["no_such_builtin"]
    ));
}

#[test]
fn test_load_mqc_verifies_bytecode() {
    let bytes = rewrite(&compile("upcase()"), |sections| {
        let code = sections.iter_mut().find(|section| section.tag == CODE).unwrap();
        let mut payload = code.payload.to_vec();
        payload.truncate(payload.len() - 1);
        code.payload = Cow::Owned(payload);
    });
    assert!(matches!(
        engine().load_mqc(&bytes),
        Err(MqcError::Malformed(_) | MqcError::InvalidBytecode(_))
    ));
}

proptest! {
    #[test]
    fn test_load_mqc_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = engine().load_mqc(&bytes);
    }

    #[test]
    fn test_load_mqc_never_panics_on_corrupted_code(index in any::<prop::sample::Index>(), value in any::<u8>()) {
        let bytes = rewrite(&compile("def f(x): x + 1; | [f(1), .h1, s\"${self}\"]"), |sections| {
            let code = sections.iter_mut().find(|section| section.tag == CODE).unwrap();
            let mut payload = code.payload.to_vec();
            let position = index.index(payload.len());
            payload[position] = value;
            code.payload = Cow::Owned(payload);
        });
        let mut engine = engine();
        if let Ok(program) = engine.load_mqc(&bytes) {
            let _ = engine.eval_compiled(program.program(), crate::null_input().into_iter());
        }
    }
}

/// Expressions whose value is known when a `.mqc` file is compiled.
fn constant_expr() -> impl Strategy<Value = String> {
    let numbers = (0u32..1000)
        .prop_map(|n| n.to_string())
        .prop_recursive(3, 16, 2, |inner| {
            (inner.clone(), prop::sample::select(vec!["+", "-", "*"]), inner)
                .prop_map(|(left, op, right)| format!("({left} {op} {right})"))
        });
    let strings = "[a-z]{0,6}"
        .prop_map(|s| format!("\"{s}\""))
        .prop_recursive(2, 8, 2, |inner| {
            prop_oneof![
                (inner.clone(), inner.clone()).prop_map(|(left, right)| format!("({left} + {right})")),
                inner.prop_map(|s| format!("upcase({s})")),
            ]
        });
    prop_oneof![
        numbers.clone(),
        strings,
        prop::collection::vec(numbers, 0..4).prop_map(|items| format!("[{}]", items.join(", "))),
    ]
}

/// Names that only exist at run time, such as `--args` values.
fn runtime_name() -> impl Strategy<Value = String> {
    "rt_[a-z]{1,8}"
}

/// Uses of `NAME` that are valid when it is bound to a string.
fn runtime_use() -> impl Strategy<Value = &'static str> {
    prop::sample::select(vec![
        "NAME",
        r#"NAME + "!""#,
        "[0, NAME]",
        r#"s"${NAME}""#,
        r#"{"k": NAME}"#,
        "upcase(NAME)",
    ])
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn test_mqc_constant_lets_match_eval(expr in constant_expr(), in_module in any::<bool>()) {
        let query = if in_module {
            format!("module m: let v = {expr} end | m::v")
        } else {
            format!("let v = {expr} | v")
        };
        let expected = engine().eval(&query, crate::null_input().into_iter()).unwrap();
        let actual = run_mqc(&compile(&query), crate::null_input()).unwrap();
        prop_assert_eq!(actual, expected, "query: {}", query);
    }

    #[test]
    fn test_compile_to_mqc_is_deterministic(expr in constant_expr()) {
        let query = format!("module m: let v = {expr} end | def f(x): x; | f(m::v)");
        prop_assert_eq!(compile(&query), compile(&query));
    }

    #[test]
    fn test_compile_to_mqc_rejects_runtime_names_in_any_module_let(name in runtime_name(), usage in runtime_use()) {
        let query = format!("module m: let v = {} end | m::v", usage.replace("NAME", &name));
        let error = engine().compile_to_mqc(&query, &[]).unwrap_err();
        prop_assert!(
            matches!(&error, MqcError::ModuleLevelNotDefined { name: found, .. } if *found == name),
            "{:?}",
            error
        );
    }

    #[test]
    fn test_mqc_reads_runtime_names_outside_module_let(
        name in runtime_name(),
        usage in runtime_use(),
        value in "[a-z]{0,6}",
    ) {
        let query = format!("let v = {} | v", usage.replace("NAME", &name));
        let with_value = || {
            let engine = engine();
            engine.define_string_value(&name, &value);
            engine
        };
        let expected = with_value().eval(&query, crate::null_input().into_iter()).unwrap();
        let mut engine = with_value();
        let program = engine.load_mqc(&compile(&query)).unwrap();
        prop_assert_eq!(program.external_globals(), [name.clone()]);
        let actual = engine.eval_compiled(program.program(), crate::null_input().into_iter()).unwrap();
        prop_assert_eq!(actual, expected, "query: {}", query);
    }
}

/// Rewrites a valid file's decoded chunks, keeping its source spans.
fn rewrite_chunks(bytes: &[u8], edit: impl FnOnce(&mut [Chunk])) -> Vec<u8> {
    rewrite(bytes, |sections| {
        let source = sections.iter().find(|section| section.tag == SOURCE).unwrap();
        let (_, spans) = decode_source(&source.payload).unwrap();
        let arena = Shared::clone(&engine().token_arena);
        let tokens = spans
            .iter()
            .map(|_| {
                crate::token_alloc(
                    &arena,
                    &Shared::new(Token {
                        range: Range::default(),
                        kind: TokenKind::Eof,
                        module_id: crate::Module::TOP_LEVEL_MODULE_ID,
                    }),
                )
            })
            .collect::<Vec<_>>();
        let code = sections.iter_mut().find(|section| section.tag == CODE).unwrap();
        let mut split = code::decode(&code.payload, &tokens, arena).unwrap();
        edit(Shared::get_mut(&mut split.program.chunks).unwrap());
        code.payload = Cow::Owned(code::encode(&split).unwrap().payload);
    })
}

/// Moves a one-parameter function's argument into the `self` slot.
fn param_in_self_slot(chunks: &mut [Chunk]) {
    let callee = &mut chunks[1];
    callee.local_names.truncate(1);
    callee.local_mutable.truncate(1);
    callee.local_count = 1;
    callee.param_shape.bindings = vec![ParamBinding::Required(0)];
    callee.code = vec![OpCode::ReturnLocal(0)];
    callee.lines.truncate(1);
    retarget_exact_calls(&mut chunks[0], 1);
}

/// Drops every local slot, including `self`, from a zero-parameter function.
fn no_self_slot(chunks: &mut [Chunk]) {
    let callee = &mut chunks[1];
    callee.local_names.clear();
    callee.local_mutable.clear();
    callee.local_count = 0;
    retarget_exact_calls(&mut chunks[0], 0);
}

fn retarget_exact_calls(chunk: &mut Chunk, local_count: u16) {
    for op in &mut chunk.code {
        if let OpCode::CallStaticExact0(target) | OpCode::CallStaticExact1(target) = op {
            target.local_count = local_count;
        }
    }
}

#[rstest]
#[case::param_in_self_slot("def f(x): x; | f(1)", param_in_self_slot)]
#[case::no_self_slot("def f(): 1; | f()", no_self_slot)]
fn test_load_mqc_rejects_frame_layout_the_vm_does_not_expect(#[case] query: &str, #[case] edit: fn(&mut [Chunk])) {
    let bytes = rewrite_chunks(&compile(query), edit);
    assert!(matches!(engine().load_mqc(&bytes), Err(MqcError::InvalidBytecode(_))));
}
