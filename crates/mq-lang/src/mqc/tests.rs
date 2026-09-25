use super::*;
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

#[test]
fn test_mqc_module_error_points_at_module_source() {
    let query = r#"import "csv" | csv::csv_parse(1, 2, 3)"#;
    let bytes = compile(query);
    let expected = engine().eval(query, crate::null_input().into_iter()).unwrap_err();
    let actual = run_mqc(&bytes, crate::null_input()).unwrap_err();
    assert_eq!(actual.cause.to_string(), expected.cause.to_string());
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
