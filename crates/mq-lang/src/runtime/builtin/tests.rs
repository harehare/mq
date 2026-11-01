use crate::DictMap;

use mq_markdown::Node;
use rstest::rstest;
use smol_str::SmolStr;

use crate::io::MemIo;
#[cfg(any(feature = "file-io", feature = "http", feature = "process-io"))]
use crate::io::NativeIo;
use crate::io::SandboxedIo;

use super::*;

#[rstest]
#[case("type", vec![RuntimeValue::String(Shared::new("test".into()))].into(), Ok(RuntimeValue::String(Shared::new("string".into()))))]
#[case("len", vec![RuntimeValue::String(Shared::new("test".into()))].into(), Ok(RuntimeValue::Number(4.into())))]
// Pinned by the hash module, so a stored summary keeps its meaning.
#[case("hash", vec![RuntimeValue::String(Shared::new("hello".into()))].into(), Ok(RuntimeValue::Number(4_952_387_935_952_618i64.into())))]
#[case("hash", vec![RuntimeValue::String(Shared::new("hello".into())), RuntimeValue::Number(1.into())].into(), Ok(RuntimeValue::Number(3_919_086_197_784_246i64.into())))]
#[case("hash", vec![RuntimeValue::String(Shared::new("hello".into())), RuntimeValue::Number(0.into())].into(), Ok(RuntimeValue::Number(4_952_387_935_952_618i64.into())))]
#[case("hash", vec![RuntimeValue::String(Shared::new("hello".into())), RuntimeValue::Number(9_007_199_254_740_991i64.into())].into(), Ok(RuntimeValue::Number(502_717_636_668_730i64.into())))]
#[case("hash", vec![RuntimeValue::String(Shared::new("".into()))].into(), Ok(RuntimeValue::Number(1_075_637_845_389_024i64.into())))]
#[case("bit_length", vec![RuntimeValue::Number(0.into())].into(), Ok(RuntimeValue::Number(0.into())))]
#[case("bit_length", vec![RuntimeValue::Number(255.into())].into(), Ok(RuntimeValue::Number(8.into())))]
#[case("bit_length", vec![RuntimeValue::Number(256.into())].into(), Ok(RuntimeValue::Number(9.into())))]
#[case("bit_length", vec![RuntimeValue::Number(3.0.into())].into(), Ok(RuntimeValue::Number(2.into())))]
#[case("bit_length", vec![RuntimeValue::Number(9_007_199_254_740_991i64.into())].into(), Ok(RuntimeValue::Number(53.into())))]
#[case("bit_length", vec![RuntimeValue::Number(9_223_372_036_854_775_808.0.into())].into(), Ok(RuntimeValue::Number(64.into())))]
#[case("bit_length", vec![RuntimeValue::Number(1e30.into())].into(), Ok(RuntimeValue::Number(100.into())))]
// German sharp s: full case folding expands ß -> ss, unlike downcase.
#[case("casefold", vec![RuntimeValue::String(Shared::new("Straße".into()))].into(), Ok(RuntimeValue::String(Shared::new("strasse".into()))))]
// Turkish dotted capital İ: default (non-Turkish) fold is i + combining dot above, not plain "i".
#[case("casefold", vec![RuntimeValue::String(Shared::new("İstanbul".into()))].into(), Ok(RuntimeValue::String(Shared::new("i\u{0307}stanbul".into()))))]
// Turkish dotless small ı is left untouched by default (non-Turkish) folding.
#[case("casefold", vec![RuntimeValue::String(Shared::new("ı".into()))].into(), Ok(RuntimeValue::String(Shared::new("ı".into()))))]
#[case("unicode_normalize", vec![RuntimeValue::String(Shared::new("e\u{0301}".into())), RuntimeValue::String(Shared::new("nfc".into()))].into(), Ok(RuntimeValue::String(Shared::new("\u{00e9}".into()))))]
#[case("unicode_normalize", vec![RuntimeValue::String(Shared::new("\u{00e9}".into())), RuntimeValue::String(Shared::new("nfd".into()))].into(), Ok(RuntimeValue::String(Shared::new("e\u{0301}".into()))))]
// NFKC of the compatibility-only ellipsis (…) decomposes it to three ASCII dots.
#[case("unicode_normalize", vec![RuntimeValue::String(Shared::new("\u{2026}".into())), RuntimeValue::String(Shared::new("NFKC".into()))].into(), Ok(RuntimeValue::String(Shared::new("...".into()))))]
// Combining acute accent stays attached to its base character as a single grapheme.
#[case("graphemes", vec![RuntimeValue::String(Shared::new("e\u{0301}f".into()))].into(), Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("e\u{0301}".into())), RuntimeValue::String(Shared::new("f".into()))]))))]
// Emoji ZWJ sequence (family) is one extended grapheme cluster despite five code points.
#[case("graphemes", vec![RuntimeValue::String(Shared::new("👨\u{200d}👩\u{200d}👧".into()))].into(), Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("👨\u{200d}👩\u{200d}👧".into()))]))))]
#[case("grapheme_len", vec![RuntimeValue::String(Shared::new("e\u{0301}f".into()))].into(), Ok(RuntimeValue::Number(2.into())))]
#[case("grapheme_len", vec![RuntimeValue::String(Shared::new("👨\u{200d}👩\u{200d}👧".into()))].into(), Ok(RuntimeValue::Number(1.into())))]
#[case("token_count", vec![RuntimeValue::String(Shared::new("Hello, world!".into())), RuntimeValue::String(Shared::new("gpt-4".into()))].into(), Ok(RuntimeValue::Number(4.into())))]
#[case("token_count", vec![RuntimeValue::String(Shared::new("".into())), RuntimeValue::String(Shared::new("gpt-4".into()))].into(), Ok(RuntimeValue::Number(0.into())))]
#[case("token_count", vec![RuntimeValue::String(Shared::new("Hello, world!".into()))].into(), Ok(RuntimeValue::Number(4.into())))]
#[case("token_count", vec![RuntimeValue::String(Shared::new("".into()))].into(), Ok(RuntimeValue::Number(0.into())))]
#[case(
        "token_compress",
        vec![
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Shared::new(Node::from("hi".to_string())), None)])),
            RuntimeValue::Number(1000.into()),
        ].into(),
        Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Shared::new(Node::from("hi".to_string())), None)])))
    )]
#[case(
        "token_compress",
        vec![RuntimeValue::None, RuntimeValue::Number(100.into())].into(),
        Ok(RuntimeValue::Array(Shared::new(vec![])))
    )]
#[case(
        "token_compress",
        vec![
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Shared::new(Node::from("hi".to_string())), None)])),
            RuntimeValue::Number(1000.into()),
            RuntimeValue::String(Shared::new("gpt-4".into())),
        ].into(),
        Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Markdown(Shared::new(Node::from("hi".to_string())), None)])))
    )]
#[case(
        "token_compress",
        vec![RuntimeValue::None, RuntimeValue::Number(100.into()), RuntimeValue::String(Shared::new("gpt-4".into()))].into(),
        Ok(RuntimeValue::Array(Shared::new(vec![])))
    )]
#[case("abs", vec![RuntimeValue::Number((-10).into())].into(), Ok(RuntimeValue::Number(10.into())))]
#[case("ceil", vec![RuntimeValue::Number(3.2.into())].into(), Ok(RuntimeValue::Number(4.0.into())))]
#[case("floor", vec![RuntimeValue::Number(3.8.into())].into(), Ok(RuntimeValue::Number(3.0.into())))]
#[case("round", vec![RuntimeValue::Number(3.5.into())].into(), Ok(RuntimeValue::Number(4.0.into())))]
#[case("sin", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("cos", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(1.0.into())))]
#[case("tan", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("log", vec![RuntimeValue::Number(1.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("asin", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("acos", vec![RuntimeValue::Number(1.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("atan", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("sinh", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("cosh", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(1.0.into())))]
#[case("tanh", vec![RuntimeValue::Number(0.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("log2", vec![RuntimeValue::Number(8.0.into())].into(), Ok(RuntimeValue::Number(3.0.into())))]
#[case("cbrt", vec![RuntimeValue::Number(27.0.into())].into(), Ok(RuntimeValue::Number(3.0.into())))]
#[case("atan2", vec![RuntimeValue::Number(0.0.into()), RuntimeValue::Number(1.0.into())].into(), Ok(RuntimeValue::Number(0.0.into())))]
#[case("hypot", vec![RuntimeValue::Number(3.0.into()), RuntimeValue::Number(4.0.into())].into(), Ok(RuntimeValue::Number(5.0.into())))]
#[case("add", vec![RuntimeValue::Number(3.0.into()), RuntimeValue::Number(2.0.into())].into(), Ok(RuntimeValue::Number(5.0.into())))]
#[case("sub", vec![RuntimeValue::Number(5.0.into()), RuntimeValue::Number(3.0.into())].into(), Ok(RuntimeValue::Number(2.0.into())))]
#[case("mul", vec![RuntimeValue::Number(4.0.into()), RuntimeValue::Number(2.0.into())].into(), Ok(RuntimeValue::Number(8.0.into())))]
#[case("div", vec![RuntimeValue::Number(8.0.into()), RuntimeValue::Number(2.0.into())].into(), Ok(RuntimeValue::Number(4.0.into())))]
#[case("has", vec![DictMap::from_iter([(Ident::new("a"), RuntimeValue::Number(1.into())), (Ident::new("b"), RuntimeValue::Number(2.into()))]).into(), RuntimeValue::String(Shared::new("a".into()))].into(), Ok(RuntimeValue::Boolean(true)))]
#[case("has", vec![DictMap::from_iter([(Ident::new("a"), RuntimeValue::Number(1.into())), (Ident::new("b"), RuntimeValue::Number(2.into()))]).into(), RuntimeValue::String(Shared::new("c".into()))].into(), Ok(RuntimeValue::Boolean(false)))]
#[case("has", vec![DictMap::from_iter([(Ident::new("a"), RuntimeValue::None)]).into(), RuntimeValue::String(Shared::new("a".into()))].into(), Ok(RuntimeValue::Boolean(true)))]
#[case("has", vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])), RuntimeValue::Number(1.into())].into(), Ok(RuntimeValue::Boolean(true)))]
#[case("has", vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])), RuntimeValue::Number(5.into())].into(), Ok(RuntimeValue::Boolean(false)))]
#[case("has", vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into()), RuntimeValue::Number(3.into())])), RuntimeValue::Number((-1).into())].into(), Ok(RuntimeValue::Boolean(false)))]
#[case("has", vec![RuntimeValue::String(Shared::new("not a container".into())), RuntimeValue::String(Shared::new("key".into()))].into(), Ok(RuntimeValue::Boolean(false)))]
fn test_eval_builtin(#[case] func_name: &str, #[case] args: Args, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new(func_name);
    assert_eq!(
        eval_builtin(&RuntimeValue::None, &ident, args, &VmEnv::default()),
        expected
    );
}

#[rstest]
#[case("div", vec![RuntimeValue::Number(1.0.into()), RuntimeValue::Number(0.0.into())].into(), Error::ZeroDivision)]
#[case("unknown_func", vec![RuntimeValue::Number(1.0.into())].into(), Error::NotDefined("unknown_func".to_string(), vec![]))]
#[case("add", vec![].into(), Error::InvalidNumberOfArguments("add".to_string(), 2, 0))]
#[case("hash", vec![RuntimeValue::Number(1.into())].into(), Error::InvalidTypes("hash".to_string(), vec![RuntimeValue::Number(1.into())]))]
#[case("hash", vec![RuntimeValue::String(Shared::new("a".into())), RuntimeValue::String(Shared::new("b".into()))].into(),
        Error::InvalidTypes("hash".to_string(), vec![RuntimeValue::String(Shared::new("a".into())), RuntimeValue::String(Shared::new("b".into()))]))]
#[case("hash", vec![RuntimeValue::String(Shared::new("a".into())), RuntimeValue::Number(1.5.into())].into(),
        Error::InvalidTypes("hash".to_string(), vec![RuntimeValue::String(Shared::new("a".into())), RuntimeValue::Number(1.5.into())]))]
#[case("hash", vec![RuntimeValue::String(Shared::new("a".into())), RuntimeValue::Number(9007199254740992i64.into())].into(),
        Error::Runtime("hash: the seed must be an integer from 0 to 2^53 - 1, got 9007199254740992".to_string()))]
#[case("hash", vec![RuntimeValue::String(Shared::new("a".into())), RuntimeValue::Number((-1i64).into())].into(),
        Error::Runtime("hash: the seed must be an integer from 0 to 2^53 - 1, got -1".to_string()))]
#[case("bit_length", vec![RuntimeValue::Number((-1).into())].into(), Error::Runtime("bit_length: expected a non-negative integer, got -1".to_string()))]
#[case("bit_length", vec![RuntimeValue::Number(1.5.into())].into(), Error::Runtime("bit_length: expected a non-negative integer, got 1.5".to_string()))]
#[case("bit_length", vec![RuntimeValue::Number(f64::INFINITY.into())].into(), Error::Runtime("bit_length: expected a non-negative integer, got inf".to_string()))]
#[case("bit_length", vec![RuntimeValue::String(Shared::new("a".into()))].into(), Error::InvalidTypes("bit_length".to_string(), vec![RuntimeValue::String(Shared::new("a".into()))]))]
#[case("add", vec![RuntimeValue::Boolean(true), RuntimeValue::Number(1.0.into())].into(),
        Error::InvalidTypes("add".to_string(), vec![RuntimeValue::Boolean(true), RuntimeValue::Number(1.0.into())]))]
#[case("unicode_normalize", vec![RuntimeValue::String(Shared::new("abc".into())), RuntimeValue::String(Shared::new("bogus".into()))].into(),
        Error::Runtime("unicode_normalize: invalid normalization form `bogus`, expected one of \"nfc\", \"nfd\", \"nfkc\", \"nfkd\"".to_string()))]
fn test_eval_builtin_errors(#[case] func_name: &str, #[case] args: Args, #[case] expected_error: Error) {
    let ident = Ident::new(func_name);
    let result = eval_builtin(&RuntimeValue::None, &ident, args, &VmEnv::default());
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), expected_error);
}

#[rstest]
#[case("halt", 1)]
#[case("convert", 2)]
#[case("date_add", 3)]
fn test_builtin_func_returns_arity_error_when_called_directly(#[case] func_name: &str, #[case] expected: u8) {
    let ident = Ident::new(func_name);
    let func = get_builtin_functions(&ident).expect("builtin is registered").func;
    let args: Args = vec![].into();
    let result = func(&ident, &RuntimeValue::None, args, &VmEnv::default());
    assert_eq!(
        result.unwrap_err(),
        Error::InvalidNumberOfArguments(func_name.to_string(), expected, 0)
    );
}

#[test]
fn test_gmtime_epoch() {
    // Unix epoch (0) → 1970-01-01T00:00:00 UTC (Thursday)
    // format: [year, mon(0-11), mday, hour, min, sec, wday(0=Sun), yday(0-365)]
    let ident = Ident::new("gmtime");
    let args = vec![RuntimeValue::Number(0.into())];
    let result = eval_builtin(&RuntimeValue::None, &ident, args.into(), &VmEnv::default()).unwrap();
    assert_eq!(
        result,
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1970.into()), // year
            RuntimeValue::Number(0.into()),    // mon (Jan=0)
            RuntimeValue::Number(1.into()),    // mday
            RuntimeValue::Number(0.into()),    // hour
            RuntimeValue::Number(0.into()),    // min
            RuntimeValue::Number(0.into()),    // sec
            RuntimeValue::Number(4.into()),    // wday (Thu=4)
            RuntimeValue::Number(0.into()),    // yday
        ]))
    );
}

#[test]
fn test_gmtime_known_date() {
    // 2024-01-01T00:00:00 UTC = 1704067200 seconds
    let ident = Ident::new("gmtime");
    let args = vec![RuntimeValue::Number(1704067200_i64.into())];
    let result = eval_builtin(&RuntimeValue::None, &ident, args.into(), &VmEnv::default()).unwrap();
    assert_eq!(
        result,
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(2024.into()), // year
            RuntimeValue::Number(0.into()),    // mon (Jan=0)
            RuntimeValue::Number(1.into()),    // mday
            RuntimeValue::Number(0.into()),    // hour
            RuntimeValue::Number(0.into()),    // min
            RuntimeValue::Number(0.into()),    // sec
            RuntimeValue::Number(1.into()),    // wday (Mon=1)
            RuntimeValue::Number(0.into()),    // yday
        ]))
    );
}

#[rstest]
#[case(0, 0)]
#[case(1704067200_i64, 1704067200_i64)]
#[case(1718454645_i64, 1718454645_i64)]
fn test_mktime_roundtrip(#[case] secs: i64, #[case] expected: i64) {
    let env = VmEnv::default();
    let gmtime_ident = Ident::new("gmtime");
    let mktime_ident = Ident::new("mktime");

    let arr = eval_builtin(
        &RuntimeValue::None,
        &gmtime_ident,
        vec![RuntimeValue::Number(secs.into())].into(),
        &env,
    )
    .unwrap();
    let result = eval_builtin(&RuntimeValue::None, &mktime_ident, vec![arr].into(), &env).unwrap();
    assert_eq!(result, RuntimeValue::Number(expected.into()));
}

fn call_uuid_fn(name: &str) -> String {
    let env = VmEnv::default();
    match eval_builtin(&RuntimeValue::None, &Ident::new(name), vec![].into(), &env).unwrap() {
        RuntimeValue::String(s) => s.to_string(),
        other => panic!("{name} should return a string, got {other:?}"),
    }
}

fn assert_uuid_shape(uuid: &str, expected_version: char) {
    let parts: Vec<&str> = uuid.split('-').collect();
    assert_eq!(parts.len(), 5, "uuid should have 5 hyphen-separated groups");
    assert_eq!(
        [
            parts[0].len(),
            parts[1].len(),
            parts[2].len(),
            parts[3].len(),
            parts[4].len()
        ],
        [8, 4, 4, 4, 12]
    );
    assert!(uuid.chars().all(|c| c == '-' || c.is_ascii_hexdigit()));
    assert_eq!(parts[2].chars().next().unwrap(), expected_version, "version nibble");
    assert!(
        matches!(parts[3].chars().next().unwrap(), '8' | '9' | 'a' | 'b'),
        "variant nibble should be 10xxxxxx"
    );
}

#[test]
fn test_uuid_is_version_4() {
    assert_uuid_shape(&call_uuid_fn("uuid"), '4');
}

#[test]
fn test_uuid_v4_is_version_4() {
    assert_uuid_shape(&call_uuid_fn("uuid_v4"), '4');
}

#[test]
fn test_uuid_v7_is_version_7() {
    assert_uuid_shape(&call_uuid_fn("uuid_v7"), '7');
}

#[test]
fn test_uuid_calls_are_unique() {
    let values: std::collections::HashSet<String> = (0..200).map(|_| call_uuid_fn("uuid")).collect();
    assert_eq!(values.len(), 200, "uuid() should not repeat across calls");
}

#[test]
fn test_rand_is_in_unit_range() {
    let env = VmEnv::default();
    for _ in 0..200 {
        match eval_builtin(&RuntimeValue::None, &Ident::new("rand"), vec![].into(), &env).unwrap() {
            RuntimeValue::Number(n) => assert!((0.0..1.0).contains(&n.value()), "rand() out of [0, 1)"),
            other => panic!("rand() should return a number, got {other:?}"),
        }
    }
}

#[rstest]
#[case(1, 10)]
#[case(-5, 5)]
#[case(7, 7)]
fn test_rand_int_within_bounds(#[case] min: i64, #[case] max: i64) {
    let env = VmEnv::default();
    for _ in 0..200 {
        let result = eval_builtin(
            &RuntimeValue::None,
            &Ident::new("rand_int"),
            vec![RuntimeValue::Number(min.into()), RuntimeValue::Number(max.into())].into(),
            &env,
        )
        .unwrap();
        match result {
            RuntimeValue::Number(n) => {
                let v = n.to_int();
                assert!((min..=max).contains(&v), "rand_int({min}, {max}) produced {v}");
            }
            other => panic!("rand_int should return a number, got {other:?}"),
        }
    }
}

#[test]
fn test_rand_seeded_is_deterministic_and_in_unit_range() {
    let env = VmEnv::default();
    let call = |seed: i64| match eval_builtin(
        &RuntimeValue::None,
        &Ident::new("rand"),
        vec![RuntimeValue::Number(seed.into())].into(),
        &env,
    )
    .unwrap()
    {
        RuntimeValue::Number(n) => n.value(),
        other => panic!("rand(seed) should return a number, got {other:?}"),
    };
    let v = call(42);
    assert!((0.0..1.0).contains(&v), "rand(42) out of [0, 1)");
    assert_eq!(v, call(42), "rand(seed) should be deterministic for a fixed seed");
}

#[test]
fn test_rand_int_seeded_is_deterministic_and_within_bounds() {
    let env = VmEnv::default();
    let call = |seed: i64| match eval_builtin(
        &RuntimeValue::None,
        &Ident::new("rand_int"),
        vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(10.into()),
            RuntimeValue::Number(seed.into()),
        ]
        .into(),
        &env,
    )
    .unwrap()
    {
        RuntimeValue::Number(n) => n.to_int(),
        other => panic!("rand_int(min, max, seed) should return a number, got {other:?}"),
    };
    let v = call(7);
    assert!((1..=10).contains(&v), "rand_int(1, 10, 7) produced {v}");
    assert_eq!(
        v,
        call(7),
        "rand_int(min, max, seed) should be deterministic for a fixed seed"
    );
}

#[test]
fn test_rand_int_invalid_range_errors() {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("rand_int"),
        vec![RuntimeValue::Number(10.into()), RuntimeValue::Number(1.into())].into(),
        &env,
    );
    assert!(result.is_err(), "rand_int(10, 1) should error since min > max");
}

#[test]
fn test_random_string_uses_only_charset_chars_and_requested_length() {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("random_string"),
        vec![
            RuntimeValue::Number(12.into()),
            RuntimeValue::String(Shared::new("abc".into())),
        ]
        .into(),
        &env,
    )
    .unwrap();
    match result {
        RuntimeValue::String(s) => {
            assert_eq!(s.chars().count(), 12);
            assert!(s.chars().all(|c| "abc".contains(c)));
        }
        other => panic!("random_string should return a string, got {other:?}"),
    }
}

#[test]
fn test_random_string_zero_length_is_empty() {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("random_string"),
        vec![
            RuntimeValue::Number(0.into()),
            RuntimeValue::String(Shared::new("abc".into())),
        ]
        .into(),
        &env,
    )
    .unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new("".into())));
}

#[test]
fn test_random_string_rejects_lengths_above_the_allocation_limit() {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("random_string"),
        vec![
            RuntimeValue::Number((MAX_RANGE_SIZE as i64 + 1).into()),
            RuntimeValue::String(Shared::new("abc".into())),
        ]
        .into(),
        &env,
    );
    assert!(matches!(result, Err(Error::Runtime(message)) if message.contains("random_string length")));
}

#[test]
fn test_random_string_empty_charset_errors() {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("random_string"),
        vec![
            RuntimeValue::Number(5.into()),
            RuntimeValue::String(Shared::new("".into())),
        ]
        .into(),
        &env,
    );
    assert!(
        result.is_err(),
        "random_string(5, \"\") should error since charset is empty"
    );
}

#[test]
fn test_random_string_seeded_is_deterministic() {
    let env = VmEnv::default();
    let call = |seed: i64| match eval_builtin(
        &RuntimeValue::None,
        &Ident::new("random_string"),
        vec![
            RuntimeValue::Number(16.into()),
            RuntimeValue::String(Shared::new("abcdefghijklmnopqrstuvwxyz0123456789".into())),
            RuntimeValue::Number(seed.into()),
        ]
        .into(),
        &env,
    )
    .unwrap()
    {
        RuntimeValue::String(s) => s.to_string(),
        other => panic!("random_string should return a string, got {other:?}"),
    };
    assert_eq!(
        call(7),
        call(7),
        "random_string(len, charset, seed) should be deterministic for a fixed seed"
    );
}

#[test]
fn test_random_string_calls_are_unique() {
    let env = VmEnv::default();
    let values: std::collections::HashSet<String> = (0..200)
        .map(|_| {
            match eval_builtin(
                &RuntimeValue::None,
                &Ident::new("random_string"),
                vec![
                    RuntimeValue::Number(16.into()),
                    RuntimeValue::String(Shared::new("abcdefghijklmnopqrstuvwxyz0123456789".into())),
                ]
                .into(),
                &env,
            )
            .unwrap()
            {
                RuntimeValue::String(s) => s.to_string(),
                other => panic!("random_string should return a string, got {other:?}"),
            }
        })
        .collect();
    assert_eq!(
        values.len(),
        200,
        "random_string(16, ...) should not repeat across calls"
    );
}

#[test]
fn test_shuffle_preserves_elements() {
    let env = VmEnv::default();
    let input: Vec<RuntimeValue> = (1..=10).map(|n| RuntimeValue::Number(n.into())).collect();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("shuffle"),
        vec![RuntimeValue::Array(Shared::new(input.clone()))].into(),
        &env,
    )
    .unwrap();
    match result {
        RuntimeValue::Array(shuffled) => {
            assert_eq!(shuffled.len(), input.len());
            let mut sorted_input = input.clone();
            let mut sorted_shuffled = (*shuffled).clone();
            sorted_input.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted_shuffled.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(sorted_input, sorted_shuffled);
        }
        other => panic!("shuffle should return an array, got {other:?}"),
    }
}

#[test]
fn test_shuffle_seeded_is_deterministic() {
    let env = VmEnv::default();
    let input: Vec<RuntimeValue> = (1..=10).map(|n| RuntimeValue::Number(n.into())).collect();
    let call = |seed: i64| match eval_builtin(
        &RuntimeValue::None,
        &Ident::new("shuffle"),
        vec![
            RuntimeValue::Array(Shared::new(input.clone())),
            RuntimeValue::Number(seed.into()),
        ]
        .into(),
        &env,
    )
    .unwrap()
    {
        RuntimeValue::Array(shuffled) => (*shuffled).clone(),
        other => panic!("shuffle should return an array, got {other:?}"),
    };
    assert_eq!(
        call(3),
        call(3),
        "shuffle(arr, seed) should be deterministic for a fixed seed"
    );
}

#[test]
fn test_sample_returns_subset_without_duplicates() {
    let env = VmEnv::default();
    let input: Vec<RuntimeValue> = (1..=10).map(|n| RuntimeValue::Number(n.into())).collect();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("sample"),
        vec![
            RuntimeValue::Array(Shared::new(input.clone())),
            RuntimeValue::Number(4.into()),
        ]
        .into(),
        &env,
    )
    .unwrap();
    match result {
        RuntimeValue::Array(sampled) => {
            assert_eq!(sampled.len(), 4);
            for v in sampled.iter() {
                assert!(input.contains(v));
            }
            let mut sorted = (*sampled).clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted.dedup();
            assert_eq!(sorted.len(), 4, "sample should not contain duplicates");
        }
        other => panic!("sample should return an array, got {other:?}"),
    }
}

#[test]
fn test_sample_seeded_is_deterministic() {
    let env = VmEnv::default();
    let input: Vec<RuntimeValue> = (1..=10).map(|n| RuntimeValue::Number(n.into())).collect();
    let call = |seed: i64| match eval_builtin(
        &RuntimeValue::None,
        &Ident::new("sample"),
        vec![
            RuntimeValue::Array(Shared::new(input.clone())),
            RuntimeValue::Number(4.into()),
            RuntimeValue::Number(seed.into()),
        ]
        .into(),
        &env,
    )
    .unwrap()
    {
        RuntimeValue::Array(sampled) => (*sampled).clone(),
        other => panic!("sample should return an array, got {other:?}"),
    };
    assert_eq!(
        call(11),
        call(11),
        "sample(arr, n, seed) should be deterministic for a fixed seed"
    );
}

#[test]
fn test_sample_n_exceeds_length_errors() {
    let env = VmEnv::default();
    let input: Vec<RuntimeValue> = (1..=3).map(|n| RuntimeValue::Number(n.into())).collect();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("sample"),
        vec![RuntimeValue::Array(Shared::new(input)), RuntimeValue::Number(10.into())].into(),
        &env,
    );
    assert!(
        result.is_err(),
        "sample(arr, n) should error when n exceeds the array length"
    );
}

#[rstest]
#[case(1704067200_i64, "%Y-%m-%d", "2024-01-01")]
#[case(0_i64, "%Y-%m-%dT%H:%M:%S", "1970-01-01T00:00:00")]
#[case(1704067200_i64, "%Y", "2024")]
fn test_strftime(#[case] ts: i64, #[case] fmt: &str, #[case] expected: &str) {
    let ident = Ident::new("strftime");
    let args = vec![
        RuntimeValue::Number(ts.into()),
        RuntimeValue::String(Shared::new(fmt.into())),
    ];
    let result = eval_builtin(&RuntimeValue::None, &ident, args.into(), &VmEnv::default()).unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new(expected.into())));
}

#[rstest]
#[case("2024-01-01", "%Y-%m-%d", 1704067200_i64)]
#[case("1970-01-01T00:00:00", "%Y-%m-%dT%H:%M:%S", 0_i64)]
#[case("01/02/2024", "%m/%d/%Y", 1704153600_i64)]
fn test_strptime(#[case] date_str: &str, #[case] fmt: &str, #[case] expected: i64) {
    let ident = Ident::new("strptime");
    let args = vec![
        RuntimeValue::String(Shared::new(date_str.into())),
        RuntimeValue::String(Shared::new(fmt.into())),
    ];
    let result = eval_builtin(&RuntimeValue::None, &ident, args.into(), &VmEnv::default()).unwrap();
    assert_eq!(result, RuntimeValue::Number(expected.into()));
}

#[test]
fn test_strptime_invalid_format() {
    let ident = Ident::new("strptime");
    let args = vec![
        RuntimeValue::String(Shared::new("not-a-date".into())),
        RuntimeValue::String(Shared::new("%Y-%m-%d".into())),
    ];
    let result = eval_builtin(&RuntimeValue::None, &ident, args.into(), &VmEnv::default());
    assert!(result.is_err());
}

fn gmtime_array(secs: i64) -> RuntimeValue {
    let env = VmEnv::default();
    eval_builtin(
        &RuntimeValue::None,
        &Ident::new("gmtime"),
        vec![RuntimeValue::Number(secs.into())].into(),
        &env,
    )
    .unwrap()
}

// date_add: simple durations
#[rstest]
#[case(1704067200_i64, 60, "seconds", 1704067260_i64)]
#[case(1704067200_i64, 5, "minutes", 1704067500_i64)]
#[case(1704067200_i64, 2, "hours", 1704074400_i64)]
#[case(1704067200_i64, 1, "days", 1704153600_i64)]
#[case(1704067200_i64, -1,  "days",    1703980800_i64)]
#[case(1704067200_i64, 1, "weeks", 1704672000_i64)]
fn test_date_add_duration(#[case] base: i64, #[case] n: i64, #[case] unit: &str, #[case] expected_secs: i64) {
    let env = VmEnv::default();
    let arr = gmtime_array(base);
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_add"),
        vec![
            arr,
            RuntimeValue::Number(n.into()),
            RuntimeValue::String(Shared::new(unit.into())),
        ]
        .into(),
        &env,
    )
    .unwrap();
    // convert result array back to timestamp via mktime and compare
    let ts = eval_builtin(&RuntimeValue::None, &Ident::new("mktime"), vec![result].into(), &env).unwrap();
    assert_eq!(ts, RuntimeValue::Number(expected_secs.into()));
}

// date_add: calendar-aware month/year arithmetic
#[test]
fn test_date_add_months_end_of_month() {
    // 2024-01-31 + 1 month = 2024-02-29 (leap year)
    let env = VmEnv::default();
    let arr = gmtime_array(1706659200); // 2024-01-31T00:00:00Z
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_add"),
        vec![
            arr,
            RuntimeValue::Number(1.into()),
            RuntimeValue::String(Shared::new("months".into())),
        ]
        .into(),
        &env,
    )
    .unwrap();
    // 2024-02-29T00:00:00Z = 1709164800
    let ts = eval_builtin(&RuntimeValue::None, &Ident::new("mktime"), vec![result].into(), &env).unwrap();
    assert_eq!(ts, RuntimeValue::Number(1709164800_i64.into()));
}

#[test]
fn test_date_add_years() {
    // 2024-02-29 + 1 year = 2025-02-28 (non-leap year clamps)
    let env = VmEnv::default();
    let arr = gmtime_array(1709164800); // 2024-02-29T00:00:00Z
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_add"),
        vec![
            arr,
            RuntimeValue::Number(1.into()),
            RuntimeValue::String(Shared::new("years".into())),
        ]
        .into(),
        &env,
    )
    .unwrap();
    // 2025-02-28T00:00:00Z = 1740700800
    let ts = eval_builtin(&RuntimeValue::None, &Ident::new("mktime"), vec![result].into(), &env).unwrap();
    assert_eq!(ts, RuntimeValue::Number(1740700800_i64.into()));
}

#[test]
fn test_date_add_invalid_unit() {
    let env = VmEnv::default();
    let arr = gmtime_array(0);
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_add"),
        vec![
            arr,
            RuntimeValue::Number(1.into()),
            RuntimeValue::String(Shared::new("centuries".into())),
        ]
        .into(),
        &env,
    );
    assert!(matches!(result, Err(Error::Runtime(_))));
}

// date_diff: difference in various units
#[rstest]
#[case(1704067200_i64, 1704153600_i64, "seconds", 86400_i64)]
#[case(1704067200_i64, 1704153600_i64, "minutes", 1440_i64)]
#[case(1704067200_i64, 1704153600_i64, "hours", 24_i64)]
#[case(1704067200_i64, 1704153600_i64, "days", 1_i64)]
#[case(1704067200_i64, 1704672000_i64, "weeks", 1_i64)]
#[case(1704153600_i64, 1704067200_i64, "seconds", -86400_i64)]
fn test_date_diff(#[case] base1: i64, #[case] base2: i64, #[case] unit: &str, #[case] expected: i64) {
    let env = VmEnv::default();
    let arr1 = gmtime_array(base1);
    let arr2 = gmtime_array(base2);
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_diff"),
        vec![arr1, arr2, RuntimeValue::String(Shared::new(unit.into()))].into(),
        &env,
    )
    .unwrap();
    assert_eq!(result, RuntimeValue::Number(expected.into()));
}

#[test]
fn test_date_diff_invalid_unit() {
    let env = VmEnv::default();
    let arr = gmtime_array(0);
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_diff"),
        vec![arr.clone(), arr, RuntimeValue::String(Shared::new("months".into()))].into(),
        &env,
    );
    assert!(matches!(result, Err(Error::Runtime(_))));
}

#[test]
fn test_gmtime_invalid_type() {
    let ident = Ident::new("gmtime");
    let args = vec![RuntimeValue::String(Shared::new("not a number".into()))];
    let result = eval_builtin(&RuntimeValue::None, &ident, args.into(), &VmEnv::default());
    assert!(matches!(result, Err(Error::InvalidTypes(_, _))));
}

#[test]
fn test_mktime_invalid_input() {
    let ident = Ident::new("mktime");
    let args = vec![RuntimeValue::String(Shared::new("not an array".into()))];
    let result = eval_builtin(&RuntimeValue::None, &ident, args.into(), &VmEnv::default());
    assert!(matches!(result, Err(Error::InvalidTypes(_, _))));
}

#[test]
fn test_date_add_malformed_array_error_prefix() {
    let env = VmEnv::default();
    let bad_arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("x".into())); 8]));
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_add"),
        vec![
            bad_arr,
            RuntimeValue::Number(1.into()),
            RuntimeValue::String(Shared::new("days".into())),
        ]
        .into(),
        &env,
    );
    match result {
        Err(Error::Runtime(msg)) => assert!(msg.starts_with("date_add:"), "expected date_add prefix, got: {msg}"),
        other => panic!("expected Runtime error, got: {other:?}"),
    }
}

#[test]
fn test_date_diff_malformed_array_error_prefix() {
    let env = VmEnv::default();
    let bad_arr = RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("x".into())); 8]));
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_diff"),
        vec![
            bad_arr.clone(),
            bad_arr,
            RuntimeValue::String(Shared::new("days".into())),
        ]
        .into(),
        &env,
    );
    match result {
        Err(Error::Runtime(msg)) => assert!(msg.starts_with("date_diff:"), "expected date_diff prefix, got: {msg}"),
        other => panic!("expected Runtime error, got: {other:?}"),
    }
}

// date_relative: base is 2024-01-15T00:00:00Z (Monday), 1705276800
#[rstest]
#[case("now", 1705276800_i64)]
#[case("today", 1705276800_i64)]
#[case("yesterday", 1705190400_i64)]
#[case("tomorrow", 1705363200_i64)]
#[case("3 days ago", 1705017600_i64)]
#[case("in 2 weeks", 1706486400_i64)]
#[case("next monday", 1705881600_i64)]
#[case("last friday", 1705017600_i64)]
fn test_date_relative(#[case] input: &str, #[case] expected_secs: i64) {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_relative"),
        vec![
            RuntimeValue::Number(1705276800_i64.into()),
            RuntimeValue::String(Shared::new(input.into())),
        ]
        .into(),
        &env,
    )
    .unwrap();
    assert_eq!(result, RuntimeValue::Number(expected_secs.into()));
}

#[test]
fn test_date_relative_invalid_expression() {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_relative"),
        vec![
            RuntimeValue::Number(1705276800_i64.into()),
            RuntimeValue::String(Shared::new("not a relative date".into())),
        ]
        .into(),
        &env,
    );
    match result {
        Err(Error::Runtime(msg)) => assert!(
            msg.starts_with("date_relative:"),
            "expected date_relative prefix, got: {msg}"
        ),
        other => panic!("expected Runtime error, got: {other:?}"),
    }
}

#[test]
fn test_date_relative_invalid_types() {
    let env = VmEnv::default();
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("date_relative"),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())].into(),
        &env,
    );
    assert!(matches!(result, Err(Error::InvalidTypes(_, _))));
}

#[test]
fn test_implicit_first_arg() {
    let ident = Ident::new("starts_with");
    let first_arg = RuntimeValue::String(Shared::new("hello world".into()));
    let args = vec![RuntimeValue::String(Shared::new("hello".into()))];

    let result = eval_builtin(&first_arg, &ident, args.into(), &VmEnv::default());
    assert_eq!(result, Ok(RuntimeValue::Boolean(true)));
}

#[rstest]
#[case::code(
        Node::Code(mq_markdown::Code { value: "test".into(), lang: Some("rust".into()), fence: true, meta: None, position: None }),
        Selector::Code,
        true
    )]
#[case::inline_code(
        Node::CodeInline(mq_markdown::CodeInline { value: "test".into(), position: None }),
        Selector::InlineCode,
        true
    )]
#[case::inline_math(
        Node::MathInline(mq_markdown::MathInline { value: "test".into(), position: None }),
        Selector::InlineMath,
        true
    )]
#[case::strong(
        Node::Strong(mq_markdown::Strong { values: vec!["test".to_string().into()], position: None }),
        Selector::Strong,
        true
    )]
#[case::emphasis(
        Node::Emphasis(mq_markdown::Emphasis{ values: vec!["test".to_string().into()], position: None }),
        Selector::Emphasis,
        true
    )]
#[case::delete(
        Node::Delete(mq_markdown::Delete{ values: vec!["test".to_string().into()], position: None }),
        Selector::Delete,
        true
    )]
#[case::link(
        Node::Link(mq_markdown::Link { url: mq_markdown::Url::new("https://example.com".into()), values: Vec::new(), title: None, position: None }),
        Selector::Link,
        true
    )]
#[case::heading_matching_depth(
        Node::Heading(mq_markdown::Heading { depth: mq_markdown::HeadingDepth::H2, values: vec!["test".to_string().into()], position: None }),
        Selector::Heading(Some(2)),
        true
    )]
#[case::heading_wrong_depth(
        Node::Heading(mq_markdown::Heading { depth: mq_markdown::HeadingDepth::H2, values: vec!["test".to_string().into()], position: None }),
        Selector::Heading(Some(3)),
        false
    )]
#[case::table_cell_with_matching_row_col(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Table(Some(1), Some(2)),
        true
    )]
#[case::table_cell_with_wrong_row(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Table(Some(2), Some(2)),
        false
    )]
#[case::table_cell_with_only_row(
        Node::TableCell(mq_markdown::TableCell { row: 1, column: 2, values: vec!["test".to_string().into()], position: None }),
        Selector::Table(Some(1), None),
        true
    )]
#[case::table_header_with_no_row_col(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(None, None),
        true
    )]
#[case::table_header_with_only_row(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(Some(2), None),
        false
    )]
#[case::table_header_with_only_col(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(None, Some(3)),
        false
    )]
#[case::table_header_with_row_col(
        Node::TableAlign(mq_markdown::TableAlign { align: vec![], position: None }),
        Selector::Table(Some(1), Some(1)),
        false
    )]
#[case::list_with_matching_index_checked(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(Some(1), Some(true)),
        true
    )]
#[case::list_with_wrong_index(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(Some(2), Some(true)),
        false
    )]
#[case::list_without_index(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::List(None, None),
        true
    )]
#[case::task_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Task,
        true
    )]
#[case::task_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Task,
        true
    )]
#[case::task_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Task,
        false
    )]
#[case::todo_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Todo,
        true
    )]
#[case::todo_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Todo,
        false
    )]
#[case::todo_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Todo,
        false
    )]
#[case::done_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(true), position: None }),
        Selector::Done,
        true
    )]
#[case::done_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: Some(false), position: None }),
        Selector::Done,
        false
    )]
#[case::done_list(
        Node::List(mq_markdown::List { marker: None, start: None, spread: false, values: vec!["test".to_string().into()], ordered: false, index: 1, level: 1, checked: None, position: None }),
        Selector::Done,
        false
    )]
#[case::text(
        Node::Text(mq_markdown::Text { value: "test".into(), position: None }),
        Selector::Text,
        true
    )]
#[case::html(
        Node::Html(mq_markdown::Html { value: "<div>test</div>".into(), position: None }),
        Selector::Html,
        true
    )]
#[case::yaml(
        Node::Yaml(mq_markdown::Yaml { value: "test".into(), position: None }),
        Selector::Yaml,
        true
    )]
#[case::toml(
        Node::Toml(mq_markdown::Toml { value: "test".into(), position: None }),
        Selector::Toml,
        true
    )]
#[case::break_(
        Node::Break(mq_markdown::Break{position: None}),
        Selector::Break,
        true
    )]
#[case::image(
        Node::Image(mq_markdown::Image { alt: "".to_string(), url: "".to_string(), title: None, position: None }),
        Selector::Image,
        true
    )]
#[case::image_ref(
        Node::ImageRef(mq_markdown::ImageRef{ alt: "".to_string(), ident: "".to_string(), label: None, position: None }),
        Selector::ImageRef,
        true
    )]
#[case::footnote(
        Node::Footnote(mq_markdown::Footnote{ident: "".to_string(), values: vec!["test".to_string().into()], position: None}),
        Selector::Footnote,
        true
    )]
#[case::footnote_ref(
        Node::FootnoteRef(mq_markdown::FootnoteRef{ident: "".to_string(), label: None, position: None}),
        Selector::FootnoteRef,
        true
    )]
#[case::math(
        Node::Math(mq_markdown::Math { value: "E=mc^2".into(), position: None }),
        Selector::Math,
        true
    )]
#[case::horizontal_rule(
        Node::HorizontalRule(mq_markdown::HorizontalRule{ marker: None, position: None }),
        Selector::HorizontalRule,
        true
    )]
#[case::blockquote(
        Node::Blockquote(mq_markdown::Blockquote{ values: vec!["test".to_string().into()], position: None }),
        Selector::Blockquote,
        true
    )]
#[case::definition(
        Node::Definition(mq_markdown::Definition { ident: "id".to_string(), url: mq_markdown::Url::new("url".into()), label: None, title: None, position: None }),
        Selector::Definition,
        true
    )]
#[case::mdx_jsx_flow_element(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        true
    )]
#[case::mdx_flow_expression(
        Node::MdxFlowExpression(mq_markdown::MdxFlowExpression{ value: "value".into(), position: None }),
        Selector::MdxFlowExpression,
        true
    )]
#[case::mdx_text_expression(
        Node::MdxTextExpression(mq_markdown::MdxTextExpression{ value: "value".into(), position: None }),
        Selector::MdxTextExpression,
        true
    )]
#[case::mdx_js_esm(
        Node::MdxJsEsm(mq_markdown::MdxJsEsm{ value: "value".into(), position: None }),
        Selector::MdxJsEsm,
        true
    )]
fn test_eval_selector(#[case] node: Node, #[case] selector: Selector, #[case] expected: bool) {
    assert_eq!(!eval_selector(&node, &selector).is_none(), expected);
}

#[test]
fn test_eval_recursive_selector_with_children() {
    let node = Node::Heading(mq_markdown::Heading {
        values: vec![
            Node::Text(mq_markdown::Text {
                value: "hello".into(),
                position: None,
            }),
            Node::Link(mq_markdown::Link {
                url: mq_markdown::Url::new("url".into()),
                title: None,
                values: Vec::new(),
                position: None,
            }),
        ],
        position: None,
        depth: mq_markdown::HeadingDepth::H1,
    });
    let result = eval_selector(&node, &Selector::Recursive);
    assert_eq!(
        result,
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Markdown(
                Shared::new(Node::Text(mq_markdown::Text {
                    value: "hello".into(),
                    position: None,
                })),
                None
            ),
            RuntimeValue::Markdown(
                Shared::new(Node::Link(mq_markdown::Link {
                    url: mq_markdown::Url::new("url".into()),
                    title: None,
                    values: Vec::new(),
                    position: None,
                })),
                None
            ),
        ]))
    );
}

#[test]
fn test_eval_recursive_selector_leaf_node() {
    let node = Node::Text(mq_markdown::Text {
        value: "leaf".into(),
        position: None,
    });
    let result = eval_selector(&node, &Selector::Recursive);
    assert_eq!(result, RuntimeValue::Array(Shared::new(vec![])));
}

#[test]
fn test_eval_recursive_selector_nested() {
    let inner_text = Node::Text(mq_markdown::Text {
        value: "nested".into(),
        position: None,
    });
    let heading = Node::Heading(mq_markdown::Heading {
        values: vec![inner_text.clone()],
        position: None,
        depth: mq_markdown::HeadingDepth::H2,
    });
    let node = Node::Blockquote(mq_markdown::Blockquote {
        values: vec![heading.clone()],
        position: None,
    });
    let result = eval_selector(&node, &Selector::Recursive);
    assert_eq!(
        result,
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::new_markdown(inner_text),
            RuntimeValue::new_markdown(heading),
        ]))
    );
}

#[rstest]
#[case(ParamNum::None, 0, true)]
#[case(ParamNum::None, 1, false)]
#[case(ParamNum::Fixed(2), 2, true)]
#[case(ParamNum::Fixed(2), 1, false)]
#[case(ParamNum::Fixed(2), 3, false)]
#[case(ParamNum::Range(1, 3), 1, true)]
#[case(ParamNum::Range(1, 3), 2, true)]
#[case(ParamNum::Range(1, 3), 3, true)]
#[case(ParamNum::Range(1, 3), 0, false)]
#[case(ParamNum::Range(1, 3), 4, false)]
fn test_param_num_is_valid(#[case] param_num: ParamNum, #[case] num_args: u8, #[case] expected: bool) {
    assert_eq!(param_num.is_valid(num_args), expected);
}

#[rstest]
#[case(ParamNum::None, 0)]
#[case(ParamNum::Fixed(2), 2)]
#[case(ParamNum::Range(1, 3), 1)]
fn test_param_num_to_num(#[case] param_num: ParamNum, #[case] expected: u8) {
    assert_eq!(param_num.to_num(), expected);
}

#[rstest]
#[case(ParamNum::None, 0, false)]
#[case(ParamNum::Fixed(2), 1, true)]
#[case(ParamNum::Fixed(2), 0, false)]
#[case(ParamNum::Range(1, 3), 0, true)]
#[case(ParamNum::Range(1, 3), 1, false)]
fn test_param_num_is_missing_one_params(#[case] param_num: ParamNum, #[case] num_args: u8, #[case] expected: bool) {
    assert_eq!(param_num.is_missing_one_params(num_args), expected);
}

// Tests for Dict functions
#[rstest]
#[case(
        DictMap::from_iter([("a", RuntimeValue::Number(1.0.into())), ("b", RuntimeValue::Number(2.0.into()))]),
        DictMap::from_iter([("c", RuntimeValue::Number(3.0.into()))]),
        DictMap::from_iter([("a", RuntimeValue::Number(1.0.into())), ("b", RuntimeValue::Number(2.0.into())), ("c", RuntimeValue::Number(3.0.into()))]),
    )]
#[case(
        DictMap::from_iter([("a", RuntimeValue::Number(1.0.into()))]),
        DictMap::from_iter([("a", RuntimeValue::Number(99.0.into())), ("b", RuntimeValue::Number(2.0.into()))]),
        DictMap::from_iter([("a", RuntimeValue::Number(99.0.into())), ("b", RuntimeValue::Number(2.0.into()))]),
    )]
#[case(
        DictMap::default(),
        DictMap::from_iter([("x", RuntimeValue::String(Shared::new("hello".into())))]),
        DictMap::from_iter([("x", RuntimeValue::String(Shared::new("hello".into())))]),
    )]
#[case(
        DictMap::from_iter([("x", RuntimeValue::String(Shared::new("hello".into())))]),
        DictMap::default(),
        DictMap::from_iter([("x", RuntimeValue::String(Shared::new("hello".into())))]),
    )]
fn test_eval_builtin_add_dict(#[case] d1: DictMap, #[case] d2: DictMap, #[case] expected: DictMap) {
    let ident = Ident::new("add");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Dict(Shared::new(d1)), RuntimeValue::Dict(Shared::new(d2))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::Dict(Shared::new(expected))));
}

#[test]
fn test_eval_builtin_new_dict() {
    let ident = Ident::new("dict");
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![].into(), &VmEnv::default());
    assert!(result.is_ok());
    let map_val = result.unwrap();
    match map_val {
        RuntimeValue::Dict(map) => {
            assert_eq!(map.len(), 0);
        }
        _ => panic!("Expected Dict, got {:?}", map_val),
    }

    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String(Shared::new("key".into())),
            RuntimeValue::String(Shared::new("value".into())),
        ]))]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(
        result,
        Ok(RuntimeValue::Dict(Shared::new(DictMap::from_iter([(
            "key",
            RuntimeValue::String(Shared::new("value".into()))
        )]))))
    );
}

#[test]
fn test_eval_builtin_set_dict() {
    let ident_set = Ident::new("set");
    let initial_map = RuntimeValue::new_dict();

    let args1 = vec![
        initial_map.clone(),
        RuntimeValue::String(Shared::new("name".into())),
        RuntimeValue::String(Shared::new("Jules".into())),
    ];
    let result1 = eval_builtin(&RuntimeValue::None, &ident_set, args1.into(), &VmEnv::default());
    assert!(result1.is_ok());
    let map_val1 = result1.unwrap();
    match &map_val1 {
        RuntimeValue::Dict(map) => {
            assert_eq!(map.len(), 1);
            assert_eq!(
                map.get(&Ident::new("name")),
                Some(&RuntimeValue::String(Shared::new("Jules".into())))
            );
        }
        _ => panic!("Expected Dict, got {:?}", map_val1),
    }

    let args2 = vec![
        map_val1.clone(),
        RuntimeValue::String(Shared::new("age".into())),
        RuntimeValue::Number(30.into()),
    ];
    let result2 = eval_builtin(&RuntimeValue::None, &ident_set, args2.into(), &VmEnv::default());
    assert!(result2.is_ok());
    let map_val2 = result2.unwrap();
    match &map_val2 {
        RuntimeValue::Dict(map) => {
            assert_eq!(map.len(), 2);
            assert_eq!(
                map.get(&Ident::new("name")),
                Some(&RuntimeValue::String(Shared::new("Jules".into())))
            );
            assert_eq!(map.get(&Ident::new("age")), Some(&RuntimeValue::Number(30.into())));
        }
        _ => panic!("Expected Dict, got {:?}", map_val2),
    }

    let args3 = vec![
        map_val2.clone(),
        RuntimeValue::String(Shared::new("name".into())),
        RuntimeValue::String(Shared::new("Vincent".into())),
    ];
    let result3 = eval_builtin(&RuntimeValue::None, &ident_set, args3.into(), &VmEnv::default());
    assert!(result3.is_ok());
    let map_val3 = result3.unwrap();
    match &map_val3 {
        RuntimeValue::Dict(map) => {
            assert_eq!(map.len(), 2);
            assert_eq!(
                map.get(&Ident::new("name")),
                Some(&RuntimeValue::String(Shared::new("Vincent".into())))
            );
            assert_eq!(map.get(&Ident::new("age")), Some(&RuntimeValue::Number(30.into())));
        }
        _ => panic!("Expected Dict, got {:?}", map_val3),
    }

    let mut nested_map_data = DictMap::default();
    nested_map_data.insert(Ident::new("level"), RuntimeValue::Number(2.into()));
    let nested_map: RuntimeValue = nested_map_data.into();
    let args4 = vec![
        map_val3.clone(),
        RuntimeValue::String(Shared::new("nested".into())),
        nested_map.clone(),
    ];
    let result4 = eval_builtin(&RuntimeValue::None, &ident_set, args4.into(), &VmEnv::default());
    assert!(result4.is_ok());
    match result4.unwrap() {
        RuntimeValue::Dict(map) => {
            assert_eq!(map.len(), 3);
            assert_eq!(map.get(&Ident::new("nested")), Some(&nested_map));
        }
        _ => panic!("Expected Dict"),
    }

    let args_err1 = vec![
        RuntimeValue::String(Shared::new("not_a_map".into())),
        RuntimeValue::String(Shared::new("key".into())),
        RuntimeValue::String(Shared::new("value".into())),
    ];
    let result_err1 = eval_builtin(&RuntimeValue::None, &ident_set, args_err1.into(), &VmEnv::default());
    assert_eq!(
        result_err1,
        Err(Error::InvalidTypes(
            "set".to_string(),
            vec![
                RuntimeValue::String(Shared::new("not_a_map".into())),
                RuntimeValue::String(Shared::new("key".into())),
                RuntimeValue::String(Shared::new("value".into()))
            ]
        ))
    );

    let args_err2 = vec![
        initial_map.clone(),
        RuntimeValue::Number(123.into()),
        RuntimeValue::String(Shared::new("value".into())),
    ];
    let result_err2 = eval_builtin(&RuntimeValue::None, &ident_set, args_err2.into(), &VmEnv::default());
    assert_eq!(
        result_err2,
        Err(Error::InvalidTypes(
            "set".to_string(),
            vec![
                initial_map.clone(),
                RuntimeValue::Number(123.into()),
                RuntimeValue::String(Shared::new("value".into()))
            ]
        ))
    );
}

#[test]
fn test_eval_builtin_get_map() {
    let ident_get = Ident::new("get");
    let mut map_data = DictMap::default();
    map_data.insert("name", RuntimeValue::String(Shared::new("Jules".into())));
    map_data.insert("age", RuntimeValue::Number(30.into()));
    let map_val: RuntimeValue = map_data.into();

    let args1 = vec![map_val.clone(), RuntimeValue::String(Shared::new("name".into()))];
    let result1 = eval_builtin(&RuntimeValue::None, &ident_get, args1.into(), &VmEnv::default());
    assert_eq!(result1, Ok(RuntimeValue::String(Shared::new("Jules".into()))));

    let args2 = vec![map_val.clone(), RuntimeValue::String(Shared::new("location".into()))];
    let result2 = eval_builtin(&RuntimeValue::None, &ident_get, args2.into(), &VmEnv::default());
    assert_eq!(result2, Ok(RuntimeValue::None));

    let args_err1 = vec![
        RuntimeValue::String(Shared::new("not_a_map".into())),
        RuntimeValue::String(Shared::new("key".into())),
    ];
    let result_err1 = eval_builtin(&RuntimeValue::None, &ident_get, args_err1.into(), &VmEnv::default());
    assert_eq!(
            result_err1,
            Err(Error::Runtime(
                "cannot index string with string (arrays, strings and markdown take a number, dicts take a string or symbol)".to_string()
            ))
        );

    let args_err2 = vec![map_val.clone(), RuntimeValue::Number(123.into())];
    let result_err2 = eval_builtin(&RuntimeValue::None, &ident_get, args_err2.into(), &VmEnv::default());
    assert_eq!(
        result_err2,
        Err(Error::Runtime(
            "cannot index dict with number (arrays, strings and markdown take a number, dicts take a string or symbol)"
                .to_string()
        ))
    );
}

#[test]
fn test_eval_builtin_get_markdown_index_beyond_selector_range() {
    let ident_get = Ident::new("get");
    let values: Vec<Node> = (0..300)
        .map(|i| {
            Node::Text(mq_markdown::Text {
                value: format!("child{i}"),
                position: None,
            })
        })
        .collect();
    let parent = RuntimeValue::Markdown(
        Shared::new(Node::Strong(mq_markdown::Strong { values, position: None })),
        None,
    );

    // Selects a child past the old u8-backed Selector's 254 cap; the u32-backed
    // Selector has no trouble representing it.
    let in_range = eval_builtin(
        &RuntimeValue::None,
        &ident_get,
        vec![parent.clone(), RuntimeValue::Number(255.into())].into(),
        &VmEnv::default(),
    )
    .unwrap();
    assert_eq!(in_range.markdown_node().unwrap().value(), "child255");

    // Still degrades to None for an index that's actually out of range for the node
    // (only 300 children, so 300 itself is out of bounds), rather than erroring or
    // crashing.
    let out_of_range = eval_builtin(
        &RuntimeValue::None,
        &ident_get,
        vec![parent, RuntimeValue::Number(300.into())].into(),
        &VmEnv::default(),
    )
    .unwrap();
    assert_eq!(out_of_range.markdown_node(), None);
}

#[test]
fn test_eval_builtin_keys_dict() {
    let ident_keys = Ident::new("keys");
    let empty_map = RuntimeValue::new_dict();
    let args1 = vec![empty_map.clone()];
    let result1 = eval_builtin(&RuntimeValue::None, &ident_keys, args1.into(), &VmEnv::default());
    assert_eq!(result1, Ok(RuntimeValue::Array(Shared::new(vec![]))));

    let mut map_data = DictMap::default();
    map_data.insert("name", RuntimeValue::String(Shared::new("Jules".into())));
    map_data.insert("age", RuntimeValue::Number(30.into()));
    let map_val: RuntimeValue = map_data.into();
    let args2 = vec![map_val.clone()];
    let result2 = eval_builtin(&RuntimeValue::None, &ident_keys, args2.into(), &VmEnv::default());
    assert!(result2.is_ok());
    match result2.unwrap() {
        RuntimeValue::Array(keys_array) => {
            assert_eq!(keys_array.len(), 2);
            let keys_str: Vec<String> = Shared::unwrap_or_clone(keys_array)
                .into_iter()
                .map(|k| match k {
                    RuntimeValue::String(s) => Shared::unwrap_or_clone(s),
                    _ => panic!("Expected string key"),
                })
                .collect();
            assert_eq!(keys_str, vec!["name".to_string(), "age".to_string()]);
        }
        _ => panic!("Expected Array of keys"),
    }

    let args_err1 = vec![RuntimeValue::String(Shared::new("not_a_map".into()))];
    let result_err1 = eval_builtin(&RuntimeValue::None, &ident_keys, args_err1.into(), &VmEnv::default());
    assert_eq!(
        result_err1,
        Err(Error::InvalidTypes(
            "keys".to_string(),
            vec![RuntimeValue::String(Shared::new("not_a_map".into()))]
        ))
    );

    let args_err2 = vec![map_val.clone(), RuntimeValue::String(Shared::new("extra".into()))];
    let result_err2 = eval_builtin(&RuntimeValue::None, &ident_keys, args_err2.into(), &VmEnv::default());
    assert_eq!(
        result_err2,
        Err(Error::InvalidNumberOfArguments("keys".to_string(), 1, 2))
    );
}

#[test]
fn test_eval_builtin_values_dict() {
    let ident_values = Ident::new("values");
    let empty_map = RuntimeValue::new_dict();
    let args1 = vec![empty_map.clone()];
    let result1 = eval_builtin(&RuntimeValue::None, &ident_values, args1.into(), &VmEnv::default());
    assert_eq!(result1, Ok(RuntimeValue::Array(Shared::new(vec![]))));

    let mut map_data = DictMap::default();
    map_data.insert("name", RuntimeValue::String(Shared::new("Jules".into())));
    map_data.insert("age", RuntimeValue::Number(30.into()));
    let map_val: RuntimeValue = map_data.into();
    let args2 = vec![map_val.clone()];
    let result2 = eval_builtin(&RuntimeValue::None, &ident_values, args2.into(), &VmEnv::default());
    assert!(result2.is_ok());
    match result2.unwrap() {
        RuntimeValue::Array(values_array) => {
            assert_eq!(values_array.len(), 2);
            assert!(values_array.contains(&RuntimeValue::String(Shared::new("Jules".into()))));
            assert!(values_array.contains(&RuntimeValue::Number(30.into())));
        }
        _ => panic!("Expected Array of values"),
    }

    let args_err1 = vec![RuntimeValue::String(Shared::new("not_a_map".into()))];
    let result_err1 = eval_builtin(&RuntimeValue::None, &ident_values, args_err1.into(), &VmEnv::default());
    assert_eq!(
        result_err1,
        Err(Error::InvalidTypes(
            "values".to_string(),
            vec![RuntimeValue::String(Shared::new("not_a_map".into()))]
        ))
    );

    let args_err2 = vec![map_val.clone(), RuntimeValue::String(Shared::new("extra".into()))];
    let result_err2 = eval_builtin(&RuntimeValue::None, &ident_values, args_err2.into(), &VmEnv::default());
    assert_eq!(
        result_err2,
        Err(Error::InvalidNumberOfArguments("values".to_string(), 1, 2))
    );
}

#[rstest]
#[case::excessively_large_range(0, 2_000_000, 1)]
#[case::negative_step_large_range(10_000_000, 0, -1)]
#[case::just_over_limit(0, 1_000_000, 1)]
fn test_range_size_limit_exceeds(#[case] start: isize, #[case] end: isize, #[case] step: isize) {
    let result = generate_numeric_range(start, end, step);
    assert!(result.is_err());
    if let Err(Error::Runtime(msg)) = result {
        assert!(msg.contains("exceeds maximum allowed size"));
    } else {
        panic!("Expected Runtime error");
    }
}

#[rstest]
#[case::reasonable_range(0, 100, 1, 101)]
#[case::exactly_at_limit(0, 999_999, 1, 1_000_000)]
#[case::ends_at_isize_max(isize::MAX - 1, isize::MAX, 1, 2)]
#[case::ends_at_isize_min(isize::MIN + 1, isize::MIN, -1, 2)]
#[case::step_past_isize_max(isize::MAX - 1, isize::MAX, isize::MAX, 1)]
fn test_range_size_limit_success(
    #[case] start: isize,
    #[case] end: isize,
    #[case] step: isize,
    #[case] expected_len: usize,
) {
    let result = generate_numeric_range(start, end, step);
    assert!(result.is_ok());
    if let Ok(vec) = result {
        assert_eq!(vec.len(), expected_len);
    }
}

#[rstest]
#[case::unicode_max_range('\u{0000}', '\u{10FFFF}', Some(1))]
fn test_char_range_size_limit_exceeds(#[case] start: char, #[case] end: char, #[case] step: Option<i32>) {
    let result = generate_char_range(start, end, step);
    assert!(result.is_err());
    if let Err(Error::Runtime(msg)) = result {
        assert!(msg.contains("exceeds maximum allowed size"));
    } else {
        panic!("Expected Runtime error");
    }
}

#[rstest]
#[case::reasonable_char_range('a', 'z', None, 26)]
#[case::step_past_i32_max('a', 'z', Some(i32::MAX), 1)]
#[case::negative_step_past_i32_min('z', 'a', Some(i32::MIN), 1)]
fn test_char_range_size_limit_success(
    #[case] start: char,
    #[case] end: char,
    #[case] step: Option<i32>,
    #[case] expected_len: usize,
) {
    let result = generate_char_range(start, end, step);
    assert!(result.is_ok());
    if let Ok(vec) = result {
        assert_eq!(vec.len(), expected_len);
    }
}

#[rstest]
#[case::excessively_large_array_repeat(
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        600_000,
        "array repeat size"
    )]
#[case::just_over_limit(
        vec![RuntimeValue::Number(1.into())],
        1_000_001,
        "exceeds maximum allowed size"
    )]
fn test_repeat_array_size_limit_exceeds(
    #[case] array: Vec<RuntimeValue>,
    #[case] n: usize,
    #[case] expected_msg: &str,
) {
    let mut value = RuntimeValue::Array(Shared::new(array));
    let result = repeat(&mut value, n);
    assert!(result.is_err());
    if let Err(Error::Runtime(msg)) = result {
        assert!(msg.contains(expected_msg));
    } else {
        panic!("Expected Runtime error for array repeat");
    }
}

#[rstest]
#[case::reasonable_array_repeat(
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        10,
        20
    )]
#[case::exactly_at_limit(
        vec![RuntimeValue::Number(1.into())],
        1_000_000,
        1_000_000
    )]
fn test_repeat_array_size_limit_success(
    #[case] array: Vec<RuntimeValue>,
    #[case] n: usize,
    #[case] expected_len: usize,
) {
    let mut value = RuntimeValue::Array(Shared::new(array));
    let result = repeat(&mut value, n);
    assert!(result.is_ok());
    if let Ok(RuntimeValue::Array(vec)) = result {
        assert_eq!(vec.len(), expected_len);
    } else {
        panic!("Expected successful array repeat");
    }
}

#[rstest]
#[case::excessively_large_string_repeat("test", 300_000, "string repeat size")]
fn test_repeat_string_size_limit_exceeds(#[case] string: &str, #[case] n: usize, #[case] expected_msg: &str) {
    let mut value = RuntimeValue::String(Shared::new(string.to_string()));
    let result = repeat(&mut value, n);
    assert!(result.is_err());
    if let Err(Error::Runtime(msg)) = result {
        assert!(msg.contains(expected_msg));
    } else {
        panic!("Expected Runtime error for string repeat");
    }
}

#[rstest]
#[case::reasonable_string_repeat("test", 10, 40)]
fn test_repeat_string_size_limit_success(#[case] string: &str, #[case] n: usize, #[case] expected_len: usize) {
    let mut value = RuntimeValue::String(Shared::new(string.to_string()));
    let result = repeat(&mut value, n);
    assert!(result.is_ok());
    if let Ok(RuntimeValue::String(s)) = result {
        assert_eq!(s.len(), expected_len);
    } else {
        panic!("Expected successful string repeat");
    }
}

#[rstest]
#[case::set_array("set", vec![RuntimeValue::empty_array(), RuntimeValue::Number((MAX_RANGE_SIZE as i64).into()), RuntimeValue::Number(1.into())])]
#[case::insert_array("insert", vec![RuntimeValue::empty_array(), RuntimeValue::Number((MAX_RANGE_SIZE as i64).into()), RuntimeValue::Number(1.into())])]
#[case::insert_string("insert", vec![RuntimeValue::String(Shared::new("".into())), RuntimeValue::Number((MAX_RANGE_SIZE as i64).into()), RuntimeValue::String(Shared::new("x".into()))])]
#[case::del_array("del", vec![RuntimeValue::empty_array(), RuntimeValue::Number((MAX_RANGE_SIZE as i64).into())])]
#[case::del_string("del", vec![RuntimeValue::String(Shared::new("".into())), RuntimeValue::Number((MAX_RANGE_SIZE as i64).into())])]
fn collection_mutators_reject_oversized_indices(#[case] name: &str, #[case] args: Vec<RuntimeValue>) {
    let env = VmEnv::default();
    let result = eval_builtin(&RuntimeValue::None, &Ident::new(name), args.into(), &env);
    assert!(matches!(result, Err(Error::Runtime(message)) if message.contains("index")));
}

#[rstest]
#[case::simple_no_header(
        "a,b,c\n1,2,3\n4,5,6",
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("a".to_string())),
                RuntimeValue::String(Shared::new("b".to_string())),
                RuntimeValue::String(Shared::new("c".to_string())),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("1".to_string())),
                RuntimeValue::String(Shared::new("2".to_string())),
                RuntimeValue::String(Shared::new("3".to_string())),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("4".to_string())),
                RuntimeValue::String(Shared::new("5".to_string())),
                RuntimeValue::String(Shared::new("6".to_string())),
            ])),
        ])))
    )]
#[case::single_row_no_header(
        "x,y",
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("x".to_string())),
                RuntimeValue::String(Shared::new("y".to_string())),
            ])),
        ])))
    )]
#[case::empty_no_header(
        "",
        Ok(RuntimeValue::Array(Shared::new(vec![])))
    )]
#[case::ragged_rows_no_header(
        "a,b,c\n1,2\n3,4,5,6",
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("a".to_string())),
                RuntimeValue::String(Shared::new("b".to_string())),
                RuntimeValue::String(Shared::new("c".to_string())),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("1".to_string())),
                RuntimeValue::String(Shared::new("2".to_string())),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("3".to_string())),
                RuntimeValue::String(Shared::new("4".to_string())),
                RuntimeValue::String(Shared::new("5".to_string())),
                RuntimeValue::String(Shared::new("6".to_string())),
            ])),
        ])))
    )]
fn test_csv_parse_no_header(#[case] csv: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("_csv_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(csv.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::simple_with_header(
        "name,age\nAlice,30\nBob,25",
        {
            let mut alice = DictMap::default();
            alice.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Alice".to_string())));
            alice.insert(Ident::new("age"), RuntimeValue::String(Shared::new("30".to_string())));
            let mut bob = DictMap::default();
            bob.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Bob".to_string())));
            bob.insert(Ident::new("age"), RuntimeValue::String(Shared::new("25".to_string())));
            Ok(RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(alice)),
                RuntimeValue::Dict(Shared::new(bob)),
            ])))
        }
    )]
#[case::single_row_with_header(
        "id,value\n1,hello",
        {
            let mut row = DictMap::default();
            row.insert(Ident::new("id"), RuntimeValue::String(Shared::new("1".to_string())));
            row.insert(Ident::new("value"), RuntimeValue::String(Shared::new("hello".to_string())));
            Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row))])))
        }
    )]
#[case::quoted_fields_with_header(
        "name,note\n\"Doe, Jane\",\"says \"\"hi\"\"\"",
        {
            let mut row = DictMap::default();
            row.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Doe, Jane".to_string())));
            row.insert(Ident::new("note"), RuntimeValue::String(Shared::new("says \"hi\"".to_string())));
            Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row))])))
        }
    )]
#[case::ragged_rows_with_header(
        "a,b,c\n1,2\n3,4,5,6",
        {
            let mut short_row = DictMap::default();
            short_row.insert(Ident::new("a"), RuntimeValue::String(Shared::new("1".to_string())));
            short_row.insert(Ident::new("b"), RuntimeValue::String(Shared::new("2".to_string())));
            short_row.insert(Ident::new("c"), RuntimeValue::String(Shared::new("".to_string())));
            let mut long_row = DictMap::default();
            long_row.insert(Ident::new("a"), RuntimeValue::String(Shared::new("3".to_string())));
            long_row.insert(Ident::new("b"), RuntimeValue::String(Shared::new("4".to_string())));
            long_row.insert(Ident::new("c"), RuntimeValue::String(Shared::new("5".to_string())));
            Ok(RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(short_row)),
                RuntimeValue::Dict(Shared::new(long_row)),
            ])))
        }
    )]
fn test_csv_parse_with_header(#[case] csv: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("_csv_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::String(Shared::new(csv.to_string())),
            RuntimeValue::String(Shared::new(",".to_string())),
            RuntimeValue::Boolean(true),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::tsv_no_header(
        "a\tb\tc\n1\t2\t3",
        "\t",
        false,
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("a".to_string())),
                RuntimeValue::String(Shared::new("b".to_string())),
                RuntimeValue::String(Shared::new("c".to_string())),
            ])),
            RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("1".to_string())),
                RuntimeValue::String(Shared::new("2".to_string())),
                RuntimeValue::String(Shared::new("3".to_string())),
            ])),
        ])))
    )]
#[case::tsv_with_header(
        "name\tage\nAlice\t30",
        "\t",
        true,
        {
            let mut row = DictMap::default();
            row.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Alice".to_string())));
            row.insert(Ident::new("age"), RuntimeValue::String(Shared::new("30".to_string())));
            Ok(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row))])))
        }
    )]
fn test_csv_parse_custom_delimiter(
    #[case] csv: &str,
    #[case] delimiter: &str,
    #[case] has_header: bool,
    #[case] expected: Result<RuntimeValue, Error>,
) {
    let ident = Ident::new("_csv_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::String(Shared::new(csv.to_string())),
            RuntimeValue::String(Shared::new(delimiter.to_string())),
            RuntimeValue::Boolean(has_header),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::invalid_type_number(RuntimeValue::Number(42.into()))]
#[case::invalid_type_bool(RuntimeValue::Boolean(false))]
fn test_csv_parse_invalid_arg_type(#[case] invalid_arg: RuntimeValue) {
    let ident = Ident::new("_csv_parse");
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![invalid_arg].into(), &VmEnv::default());
    assert!(result.is_err());
}

#[rstest]
#[case::simple_object(
        r#"{"key": "value"}"#,
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("key"), RuntimeValue::String(Shared::new("value".to_string())));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::array(
        r#"[1, 2, 3]"#,
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])))
    )]
#[case::nested(
        r#"{"a": [true, null], "b": {"c": 1.2}}"#,
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("a"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Boolean(true),
                RuntimeValue::NONE,
            ])));
            let mut inner = DictMap::default();
            inner.insert(Ident::new("c"), RuntimeValue::Number(1.2.into()));
            map.insert(Ident::new("b"), RuntimeValue::Dict(Shared::new(inner)));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::string(r#""hello""#, Ok(RuntimeValue::String(Shared::new("hello".to_string()))))]
#[case::number(r#"42"#, Ok(RuntimeValue::Number(42.into())))]
#[case::boolean(r#"false"#, Ok(RuntimeValue::Boolean(false)))]
#[case::null(r#"null"#, Ok(RuntimeValue::NONE))]
fn test_json_parse(#[case] json: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("_json_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(json.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::invalid_json(r#"{"key": "value""#)]
#[case::invalid_type(RuntimeValue::Number(1.into()))]
fn test_json_parse_error(#[case] input: impl Into<RuntimeValue>) {
    let ident = Ident::new("_json_parse");
    let arg: RuntimeValue = match input.into() {
        RuntimeValue::Number(n) => RuntimeValue::Number(n),
        s => RuntimeValue::String(Shared::new(s.to_string())),
    };
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![arg].into(), &VmEnv::default());
    assert!(result.is_err());
}

#[rstest]
#[case::mapping(
        "key: value",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("key"), RuntimeValue::String(Shared::new("value".to_string())));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::sequence(
        "- 1\n- 2\n- 3",
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(2.into()),
            RuntimeValue::Number(3.into()),
        ])))
    )]
#[case::nested(
        "a:\n  b: 42",
        {
            let mut inner = DictMap::default();
            inner.insert(Ident::new("b"), RuntimeValue::Number(42.into()));
            let mut map = DictMap::default();
            map.insert(Ident::new("a"), RuntimeValue::Dict(Shared::new(inner)));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::boolean(
        "flag: true",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("flag"), RuntimeValue::Boolean(true));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::null(
        "value: null",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("value"), RuntimeValue::NONE);
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::float(
        "ratio: 1.5",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("ratio"), RuntimeValue::Number(1.5.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::multi_document(
        "a: 1\n---\nb: 2\n",
        {
            let mut first = DictMap::default();
            first.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
            let mut second = DictMap::default();
            second.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
            Ok(RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Dict(Shared::new(first)),
                RuntimeValue::Dict(Shared::new(second)),
            ])))
        }
    )]
#[case::infinity(
        "up: .inf\ndown: -.inf\nplus: +.Inf",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("up"), RuntimeValue::Number(f64::INFINITY.into()));
            map.insert(Ident::new("down"), RuntimeValue::Number(f64::NEG_INFINITY.into()));
            map.insert(Ident::new("plus"), RuntimeValue::Number(f64::INFINITY.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::non_string_keys(
        "1: a\ntrue: b\n1.5: c\nnull: d",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("1"), RuntimeValue::String(Shared::new("a".to_string())));
            map.insert(Ident::new("true"), RuntimeValue::String(Shared::new("b".to_string())));
            map.insert(Ident::new("1.5"), RuntimeValue::String(Shared::new("c".to_string())));
            map.insert(Ident::new("null"), RuntimeValue::String(Shared::new("d".to_string())));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
fn test_yaml_parse(#[case] yaml: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("_yaml_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(yaml.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::invalid_type(RuntimeValue::Number(1.into()))]
fn test_yaml_parse_error(#[case] input: impl Into<RuntimeValue>) {
    let ident = Ident::new("_yaml_parse");
    let arg: RuntimeValue = match input.into() {
        RuntimeValue::Number(n) => RuntimeValue::Number(n),
        s => RuntimeValue::String(Shared::new(s.to_string())),
    };
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![arg].into(), &VmEnv::default());
    assert!(result.is_err());
}

#[rstest]
#[case::simple_kv(
        "a: 1\nb: 2",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
            map.insert(Ident::new("b"), RuntimeValue::Number(2.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::nested_indent(
        "parent:\n  child: value",
        {
            let mut child_map = DictMap::default();
            child_map.insert(Ident::new("child"), RuntimeValue::String(Shared::new("value".to_string())));
            let mut parent_map = DictMap::default();
            parent_map.insert(Ident::new("parent"), RuntimeValue::Dict(Shared::new(child_map)));
            Ok(RuntimeValue::Dict(Shared::new(parent_map)))
        }
    )]
#[case::tabular_data(
        "hikes[2]{id,name}:\n  1,Blue Lake\n  2,Ridge Trail",
        {
            let mut row1 = DictMap::default();
            row1.insert(Ident::new("id"), RuntimeValue::Number(1.into()));
            row1.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Blue Lake".to_string())));
            let mut row2 = DictMap::default();
            row2.insert(Ident::new("id"), RuntimeValue::Number(2.into()));
            row2.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Ridge Trail".to_string())));
            let mut map = DictMap::default();
            map.insert(Ident::new("hikes"), RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(row1)), RuntimeValue::Dict(Shared::new(row2))])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::inline_array(
        "items[3]: 1, 2, 3",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("items"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
                RuntimeValue::Number(3.into()),
            ])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::expanded_array(
        "items[2]:\n  - 1\n  - 2",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("items"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Number(2.into()),
            ])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::primitives(
        "s: \"string\"\nb: true\nn: null\nf: false",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("s"), RuntimeValue::String(Shared::new("string".to_string())));
            map.insert(Ident::new("b"), RuntimeValue::TRUE);
            map.insert(Ident::new("n"), RuntimeValue::NONE);
            map.insert(Ident::new("f"), RuntimeValue::FALSE);
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
fn test_toon_parse(#[case] toon: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("_toon_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(toon.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

// Each case is a lone `RuntimeValue`: multi-key dicts are exercised separately in
// `test_toon_stringify_round_trip` instead of here, since `RuntimeValue::Dict` (a
// `BTreeMap<Ident, _>`) orders keys by interned symbol id, which isn't stable across
// a test binary run and would make an exact-string assertion on more than one key flaky.
#[rstest]
#[case::string(RuntimeValue::String(Shared::new("hello".to_string())), "hello")]
#[case::number(RuntimeValue::Number(42.into()), "42")]
#[case::bool_true(RuntimeValue::TRUE, "true")]
#[case::bool_false(RuntimeValue::FALSE, "false")]
#[case::none(RuntimeValue::NONE, "null")]
#[case::single_key_dict(
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Alice".to_string())));
            RuntimeValue::Dict(Shared::new(map))
        },
        "name: Alice"
    )]
#[case::array_of_primitives(
        RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())])),
        "[2]: 1,2"
    )]
#[case::empty_array(RuntimeValue::Array(Shared::new(vec![])), "[]")]
#[case::empty_dict(RuntimeValue::Dict(Shared::new(DictMap::default())), "")]
#[case::empty_string_needs_quoting(RuntimeValue::String(Shared::new("".to_string())), "\"\"")]
#[case::numeric_like_string_needs_quoting(RuntimeValue::String(Shared::new("123".to_string())), "\"123\"")]
#[case::keyword_like_string_needs_quoting(RuntimeValue::String(Shared::new("true".to_string())), "\"true\"")]
#[case::string_with_colon_needs_quoting(RuntimeValue::String(Shared::new("a:b".to_string())), "\"a:b\"")]
#[case::string_with_delimiter_needs_quoting(RuntimeValue::String(Shared::new("a,b".to_string())), "\"a,b\"")]
#[case::string_starting_with_dash_needs_quoting(RuntimeValue::String(Shared::new("-x".to_string())), "\"-x\"")]
#[case::integer_above_i64_max(RuntimeValue::Number(2f64.powi(63).into()), "9223372036854775808")]
#[case::integer_above_i64_max_up_to_u64(RuntimeValue::Number(1e19.into()), "10000000000000000000")]
#[case::integer_i64_min(RuntimeValue::Number((i64::MIN as f64).into()), "-9223372036854775808")]
fn test_toon_stringify(#[case] input: RuntimeValue, #[case] expected: &str) {
    let ident = Ident::new("_toon_stringify");
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![input].into(), &VmEnv::default());
    assert_eq!(result, Ok(RuntimeValue::String(Shared::new(expected.to_string()))));
}

fn toon_tabular_row(id: i64, name: &str) -> RuntimeValue {
    let mut map = DictMap::default();
    map.insert(Ident::new("id"), RuntimeValue::Number(id.into()));
    map.insert(Ident::new("name"), RuntimeValue::String(Shared::new(name.to_string())));
    RuntimeValue::Dict(Shared::new(map))
}

// Structural round trip (stringify then re-parse) rather than an exact string:
// `RuntimeValue`'s `Dict` equality ignores key order, so this stays robust regardless
// of how the global symbol interner happens to order a multi-key dict's fields.
#[rstest]
#[case::tabular_array(RuntimeValue::Array(Shared::new(vec![
        toon_tabular_row(1, "Blue Lake"),
        toon_tabular_row(2, "Ridge Trail"),
    ])))]
#[case::nested_dict({
        let mut inner = DictMap::default();
        inner.insert(Ident::new("inner"), RuntimeValue::Number(1.into()));
        let mut outer = DictMap::default();
        outer.insert(Ident::new("outer"), RuntimeValue::Dict(Shared::new(inner)));
        RuntimeValue::Dict(Shared::new(outer))
    })]
#[case::non_uniform_array(RuntimeValue::Array(Shared::new(vec![
        RuntimeValue::Number(1.into()),
        toon_tabular_row(2, "Ridge Trail"),
        RuntimeValue::String(Shared::new("text".to_string())),
    ])))]
fn test_toon_stringify_round_trip(#[case] original: RuntimeValue) {
    let env = VmEnv::default();

    let ident_stringify = Ident::new("_toon_stringify");
    let stringified = eval_builtin(
        &RuntimeValue::None,
        &ident_stringify,
        vec![original.clone()].into(),
        &env,
    )
    .unwrap();
    let RuntimeValue::String(toon_str) = stringified else {
        panic!("expected a string result");
    };

    let ident_parse = Ident::new("_toon_parse");
    let round_tripped = eval_builtin(
        &RuntimeValue::None,
        &ident_parse,
        vec![RuntimeValue::String(toon_str)].into(),
        &env,
    )
    .unwrap();

    assert_eq!(round_tripped, original);
}

#[rstest]
#[case::simple_kv(
        "name = \"Alice\"\nage = 30",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Alice".to_string())));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::boolean(
        "enabled = true\ndisabled = false",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("enabled"), RuntimeValue::Boolean(true));
            map.insert(Ident::new("disabled"), RuntimeValue::Boolean(false));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::nested_table(
        "[server]\nhost = \"localhost\"\nport = 8080",
        {
            let mut inner = DictMap::default();
            inner.insert(Ident::new("host"), RuntimeValue::String(Shared::new("localhost".to_string())));
            inner.insert(Ident::new("port"), RuntimeValue::Number(8080.into()));
            let mut map = DictMap::default();
            map.insert(Ident::new("server"), RuntimeValue::Dict(Shared::new(inner)));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::array(
        "tags = [\"rust\", \"toml\"]",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("tags"), RuntimeValue::Array(Shared::new(vec![
                RuntimeValue::String(Shared::new("rust".to_string())),
                RuntimeValue::String(Shared::new("toml".to_string())),
            ])));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::datetime(
        "d = 1979-05-27T07:32:00Z",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("d"), RuntimeValue::String(Shared::new("1979-05-27T07:32:00Z".to_string())));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
#[case::inf(
        "f = inf",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("f"), RuntimeValue::Number(f64::INFINITY.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
fn test_toml_parse(#[case] toml: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("_toml_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(toml.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::invalid_toml("name = ")]
fn test_toml_parse_error(#[case] input: &str) {
    let ident = Ident::new("_toml_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(input.to_string()))].into(),
        &VmEnv::default(),
    );
    assert!(result.is_err());
}

#[rstest]
#[case::invalid_type(RuntimeValue::Number(1.into()))]
fn test_toml_parse_invalid_type(#[case] input: RuntimeValue) {
    let ident = Ident::new("_toml_parse");
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![input].into(), &VmEnv::default());
    assert!(result.is_err());
}

#[rstest]
#[case::simple_map(
        // {"name": "Alice", "age": 30}
        "omRuYW1lZUFsaWNlY2FnZRge",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Alice".to_string())));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
fn test_cbor_parse(#[case] input: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("_cbor_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(input.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::invalid_base64("not-valid-base64!!!")]
#[case::invalid_cbor("aGVsbG8=")]
fn test_cbor_parse_error(#[case] input: &str) {
    let ident = Ident::new("_cbor_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(input.to_string()))].into(),
        &VmEnv::default(),
    );
    assert!(result.is_err());
}

#[rstest]
#[case::invalid_type(RuntimeValue::Number(1.into()))]
fn test_cbor_parse_invalid_type(#[case] input: RuntimeValue) {
    let ident = Ident::new("_cbor_parse");
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![input].into(), &VmEnv::default());
    assert!(result.is_err());
}

#[rstest]
#[case::simple_map(
        // {"name": "Alice", "age": 30} encoded as CBOR then base64
        "omRuYW1lZUFsaWNlY2FnZRge",
        {
            let mut map = DictMap::default();
            map.insert(Ident::new("name"), RuntimeValue::String(Shared::new("Alice".to_string())));
            map.insert(Ident::new("age"), RuntimeValue::Number(30.into()));
            Ok(RuntimeValue::Dict(Shared::new(map)))
        }
    )]
fn test_cbor_stringify_roundtrip(#[case] base64_input: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let env = VmEnv::default();

    // parse
    let ident_parse = Ident::new("_cbor_parse");
    let parsed = eval_builtin(
        &RuntimeValue::None,
        &ident_parse,
        vec![RuntimeValue::String(Shared::new(base64_input.to_string()))].into(),
        &env,
    );
    assert!(parsed.is_ok());
    assert_eq!(parsed.as_ref().ok(), expected.as_ref().ok());

    // stringify
    let ident_stringify = Ident::new("_cbor_stringify");
    let bytes_result = eval_builtin(
        &RuntimeValue::None,
        &ident_stringify,
        vec![parsed.unwrap()].into(),
        &env,
    );
    assert!(bytes_result.is_ok());
    assert!(matches!(bytes_result.unwrap(), RuntimeValue::Bytes(_)));
}

#[test]
fn test_cbor_parse_from_bytes() {
    // {"name": "Alice"} as raw CBOR bytes
    let cbor_bytes = base64::engine::general_purpose::STANDARD
        .decode("oWRuYW1lZUFsaWNl")
        .unwrap();
    let ident = Ident::new("_cbor_parse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(cbor_bytes.into())].into(),
        &VmEnv::default(),
    );
    assert!(result.is_ok());
    let mut expected = DictMap::default();
    expected.insert(
        Ident::new("name"),
        RuntimeValue::String(Shared::new("Alice".to_string())),
    );
    assert_eq!(result.unwrap(), RuntimeValue::Dict(Shared::new(expected)));
}

#[test]
fn test_base64_bytes_input() {
    let ident = Ident::new("base64");
    let bytes = vec![0x48u8, 0x65, 0x6c, 0x6c, 0x6f]; // "Hello"
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(bytes.into())].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::String(Shared::new("SGVsbG8=".to_string()))));
}

#[rstest]
#[case::string(
        RuntimeValue::String(Shared::new("hello".to_string())),
        Ok(RuntimeValue::Bytes(Shared::new(vec![0x68, 0x65, 0x6c, 0x6c, 0x6f])))
    )]
#[case::empty_string(
        RuntimeValue::String(Shared::new("".to_string())),
        Ok(RuntimeValue::Bytes(Shared::new(vec![])))
    )]
#[case::utf8_string(
        RuntimeValue::String(Shared::new("あ".to_string())),
        Ok(RuntimeValue::Bytes(Shared::new(vec![0xe3, 0x81, 0x82])))
    )]
#[case::array_of_numbers(
        RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(0.into()),
            RuntimeValue::Number(255.into()),
            RuntimeValue::Number(128.into()),
        ])),
        Ok(RuntimeValue::Bytes(Shared::new(vec![0, 255, 128])))
    )]
#[case::bytes_identity(
        RuntimeValue::Bytes(Shared::new(vec![1, 2, 3])),
        Ok(RuntimeValue::Bytes(Shared::new(vec![1, 2, 3])))
    )]
fn test_to_bytes(#[case] input: RuntimeValue, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("to_bytes");
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![input].into(), &VmEnv::default());
    assert_eq!(result, expected);
}

#[rstest]
#[case::number(RuntimeValue::Number(42.into()))]
#[case::array_with_non_number(RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("x".to_string()))])))]
#[case::array_with_negative(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number((-1i64).into())])))]
#[case::array_with_256(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(256i64.into())])))]
#[case::array_with_fractional(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.5f64.into())])))]
#[case::array_with_nan(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(f64::NAN.into())])))]
#[case::array_with_infinity(RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(f64::INFINITY.into())])))]
fn test_to_bytes_invalid(#[case] input: RuntimeValue) {
    let ident = Ident::new("to_bytes");
    let result = eval_builtin(&RuntimeValue::None, &ident, vec![input].into(), &VmEnv::default());
    assert!(result.is_err());
}

#[test]
fn test_bytes_add() {
    let ident = Ident::new("add");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![1, 2])),
            RuntimeValue::Bytes(Shared::new(vec![3, 4])),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::Bytes(Shared::new(vec![1, 2, 3, 4]))));
}

#[test]
fn test_bytes_reverse() {
    let ident = Ident::new("reverse");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(Shared::new(vec![1, 2, 3]))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::Bytes(Shared::new(vec![3, 2, 1]))));
}

#[test]
fn test_bytes_slice() {
    let ident = Ident::new("slice");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![10, 20, 30, 40, 50])),
            RuntimeValue::Number(1.into()),
            RuntimeValue::Number(4.into()),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::Bytes(Shared::new(vec![20, 30, 40]))));
}

#[test]
fn test_md5_bytes_input() {
    let ident = Ident::new("md5");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(Shared::new(b"hello".to_vec()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(
        result,
        Ok(RuntimeValue::String(Shared::new(
            "5d41402abc4b2a76b9719d911017c592".to_string()
        )))
    );
}

#[test]
fn test_sha256_bytes_input() {
    let ident = Ident::new("sha256");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(Shared::new(b"hello".to_vec()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(
        result,
        Ok(RuntimeValue::String(Shared::new(
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".to_string()
        )))
    );
}

#[test]
fn test_sha512_string_input() {
    let ident = Ident::new("sha512");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new("hello".to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(
            result,
            Ok(RuntimeValue::String(Shared::new(
                "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043".to_string()
            )))
        );
}

#[test]
fn test_sha512_bytes_input() {
    let ident = Ident::new("sha512");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(Shared::new(b"hello".to_vec()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(
            result,
            Ok(RuntimeValue::String(Shared::new(
                "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043".to_string()
            )))
        );
}

#[rstest]
#[case::lowercase("deadbeef", Ok(RuntimeValue::Bytes(Shared::new(vec![0xde, 0xad, 0xbe, 0xef]))))]
#[case::uppercase("DEADBEEF", Ok(RuntimeValue::Bytes(Shared::new(vec![0xde, 0xad, 0xbe, 0xef]))))]
#[case::empty("", Ok(RuntimeValue::Bytes(Shared::new(vec![]))))]
fn test_from_hex(#[case] input: &str, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("from_hex");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(input.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[rstest]
#[case::odd_length("abc", true)]
#[case::invalid_chars("zzzz", true)]
fn test_from_hex_invalid(#[case] input: &str, #[case] is_err: bool) {
    let ident = Ident::new("from_hex");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::String(Shared::new(input.to_string()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result.is_err(), is_err);
}

#[rstest]
#[case::basic(vec![0xde, 0xad, 0xbe, 0xef], Ok(RuntimeValue::String(Shared::new("deadbeef".to_string()))))]
#[case::empty(vec![], Ok(RuntimeValue::String(Shared::new("".to_string()))))]
#[case::zero_ff(vec![0x00, 0xff], Ok(RuntimeValue::String(Shared::new("00ff".to_string()))))]
#[case::all_zeros(vec![0x00, 0x00], Ok(RuntimeValue::String(Shared::new("0000".to_string()))))]
fn test_to_hex(#[case] input: Vec<u8>, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("to_hex");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(input.into())].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[test]
fn test_to_hex_roundtrip() {
    let env = VmEnv::default();
    let original = vec![0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef];
    let hex = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("to_hex"),
        vec![RuntimeValue::Bytes(Shared::new(original.clone()))].into(),
        &env,
    )
    .unwrap();
    let roundtripped = eval_builtin(&RuntimeValue::None, &Ident::new("from_hex"), vec![hex].into(), &env).unwrap();
    assert_eq!(roundtripped, RuntimeValue::Bytes(original.into()));
}

#[rstest]
#[case::empty(vec![], Ok(RuntimeValue::String(Shared::new("00000000".to_string()))))]
#[case::short(
        b"hi".to_vec(),
        Ok(RuntimeValue::String(Shared::new("00000000  68 69                                             |hi|".to_string())))
    )]
fn test_hexdump(#[case] input: Vec<u8>, #[case] expected: Result<RuntimeValue, Error>) {
    let ident = Ident::new("hexdump");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(input.into())].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, expected);
}

#[test]
fn test_hexdump_none() {
    let result = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("hexdump"),
        vec![RuntimeValue::None].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::NONE));
}

#[test]
fn test_utf8_valid() {
    let ident = Ident::new("utf8");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(Shared::new(b"hello".to_vec()))].into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::String(Shared::new("hello".to_string()))));
}

#[test]
fn test_utf8_invalid() {
    let ident = Ident::new("utf8");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![RuntimeValue::Bytes(Shared::new(vec![0xff, 0xfe]))].into(),
        &VmEnv::default(),
    );
    assert!(result.is_err());
}

#[test]
fn test_decode_shift_jis() {
    let ident = Ident::new("decode");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![0x82, 0xa0])),
            RuntimeValue::String(Shared::new("shift_jis".to_string())),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::String(Shared::new("あ".to_string()))));
}

#[test]
fn test_decode_unknown_label() {
    let ident = Ident::new("decode");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![0x00])),
            RuntimeValue::String(Shared::new("not-a-real-encoding".to_string())),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert!(result.is_err());
}

#[test]
fn test_decode_invalid_bytes() {
    let ident = Ident::new("decode");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![0xff, 0xff])),
            RuntimeValue::String(Shared::new("shift_jis".to_string())),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert!(result.is_err());
}

#[test]
fn test_encode_shift_jis() {
    let ident = Ident::new("encode");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::String(Shared::new("あ".to_string())),
            RuntimeValue::String(Shared::new("shift_jis".to_string())),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::Bytes(Shared::new(vec![0x82, 0xa0]))));
}

#[test]
fn test_encode_decode_roundtrip() {
    let env = VmEnv::default();
    let encoded = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("encode"),
        vec![
            RuntimeValue::String(Shared::new("こんにちは".to_string())),
            RuntimeValue::String(Shared::new("euc-jp".to_string())),
        ]
        .into(),
        &env,
    )
    .unwrap();
    let decoded = eval_builtin(
        &RuntimeValue::None,
        &Ident::new("decode"),
        vec![encoded, RuntimeValue::String(Shared::new("euc-jp".to_string()))].into(),
        &env,
    );
    assert_eq!(decoded, Ok(RuntimeValue::String(Shared::new("こんにちは".to_string()))));
}

#[test]
fn test_encode_unmappable_character() {
    let ident = Ident::new("encode");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::String(Shared::new("😀".to_string())),
            RuntimeValue::String(Shared::new("shift_jis".to_string())),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert!(result.is_err());
}

#[test]
fn test_xor_basic() {
    let ident = Ident::new("xor");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![0xaa, 0xbb])),
            RuntimeValue::Bytes(Shared::new(vec![0x55, 0x44])),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::Bytes(Shared::new(vec![0xff, 0xff]))));
}

#[test]
fn test_xor_identity() {
    let ident = Ident::new("xor");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![0x01, 0x02, 0x03])),
            RuntimeValue::Bytes(Shared::new(vec![0x00, 0x00, 0x00])),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert_eq!(result, Ok(RuntimeValue::Bytes(Shared::new(vec![0x01, 0x02, 0x03]))));
}

#[test]
fn test_xor_length_mismatch() {
    let ident = Ident::new("xor");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Bytes(Shared::new(vec![0x01, 0x02])),
            RuntimeValue::Bytes(Shared::new(vec![0x01])),
        ]
        .into(),
        &VmEnv::default(),
    );
    assert!(result.is_err());
}

#[test]
fn test_diff_strings() {
    let ident = Ident::new("_diff");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::String(Shared::new("abc".into())),
            RuntimeValue::String(Shared::new("abc ".into())),
        ]
        .into(),
        &VmEnv::default(),
    );

    assert!(result.is_ok());
    if let Ok(RuntimeValue::Array(changes)) = result {
        // line-level diff: delete "abc" + insert "abc " (replace pair)
        assert_eq!(changes.len(), 2);
        if let RuntimeValue::Dict(ref m) = changes[0] {
            assert_eq!(
                m.get(&Ident::new("tag")),
                Some(&RuntimeValue::String(Shared::new("delete".into())))
            );
            assert_eq!(
                m.get(&Ident::new("value")),
                Some(&RuntimeValue::String(Shared::new("abc".into())))
            );
            assert!(m.contains_key(&Ident::new("inline")));
        } else {
            panic!("Expected Dict change");
        }
        if let RuntimeValue::Dict(ref m) = changes[1] {
            assert_eq!(
                m.get(&Ident::new("tag")),
                Some(&RuntimeValue::String(Shared::new("insert".into())))
            );
            assert_eq!(
                m.get(&Ident::new("value")),
                Some(&RuntimeValue::String(Shared::new("abc ".into())))
            );
            // inline should show the trailing space as "insert"
            if let Some(RuntimeValue::Array(inline)) = m.get(&Ident::new("inline")) {
                let last = inline.last().expect("inline should not be empty");
                if let RuntimeValue::Dict(lm) = last {
                    assert_eq!(
                        lm.get(&Ident::new("tag")),
                        Some(&RuntimeValue::String(Shared::new("insert".into())))
                    );
                    assert_eq!(
                        lm.get(&Ident::new("value")),
                        Some(&RuntimeValue::String(Shared::new(" ".into())))
                    );
                } else {
                    panic!("Expected Dict in inline");
                }
            } else {
                panic!("Expected inline Array");
            }
        } else {
            panic!("Expected Dict change");
        }
    } else {
        panic!("Expected Array result");
    }
}

#[test]
fn test_diff_arrays() {
    let ident = Ident::new("_diff");
    let result = eval_builtin(
        &RuntimeValue::None,
        &ident,
        vec![
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into())])),
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(2.into())])),
        ]
        .into(),
        &VmEnv::default(),
    );

    assert!(result.is_ok());
    if let Ok(RuntimeValue::Array(changes)) = result {
        assert_eq!(changes.len(), 2); // delete 1, insert 2
        if let RuntimeValue::Dict(ref m) = changes[0] {
            assert_eq!(
                m.get(&Ident::new("tag")),
                Some(&RuntimeValue::String(Shared::new("delete".into())))
            );
            assert_eq!(m.get(&Ident::new("value")), Some(&RuntimeValue::Number(1.into())));
            // non-string elements have no inline field
            assert!(!m.contains_key(&Ident::new("inline")));
        } else {
            panic!("Expected Dict change");
        }
        if let RuntimeValue::Dict(ref m) = changes[1] {
            assert_eq!(
                m.get(&Ident::new("tag")),
                Some(&RuntimeValue::String(Shared::new("insert".into())))
            );
            assert_eq!(m.get(&Ident::new("value")), Some(&RuntimeValue::Number(2.into())));
            assert!(!m.contains_key(&Ident::new("inline")));
        } else {
            panic!("Expected Dict change");
        }
    } else {
        panic!("Expected Array result");
    }
}

#[rstest]
#[case::single_number(vec![RuntimeValue::Number(1.into())], vec![1u8])]
#[case::multiple_numbers(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())], vec![1u8, 2u8])]
#[case::number_array(vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())]))], vec![1u8, 2u8])]
#[case::empty(vec![], vec![])]
#[case::ignores_strings(vec![RuntimeValue::String(Shared::new("x".into()))], vec![])]
fn test_collect_depth_values(#[case] args: Vec<RuntimeValue>, #[case] expected: Vec<u8>) {
    assert_eq!(collect_depth_values(&args), expected);
}

#[rstest]
#[case::single_string(vec![RuntimeValue::String(Shared::new("rust".into()))], vec!["rust".to_string()])]
#[case::multiple_strings(vec![RuntimeValue::String(Shared::new("rust".into())), RuntimeValue::String(Shared::new("go".into()))], vec!["rust".to_string(), "go".to_string()])]
#[case::string_array(vec![RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("rust".into())), RuntimeValue::String(Shared::new("go".into()))]))], vec!["rust".to_string(), "go".to_string()])]
#[case::empty(vec![], vec![])]
#[case::ignores_numbers(vec![RuntimeValue::Number(1.into())], vec![])]
fn test_collect_string_values(#[case] args: Vec<RuntimeValue>, #[case] expected: Vec<String>) {
    assert_eq!(collect_string_values(&args), expected);
}

#[rstest]
#[case::heading_depth_match(
        Node::Heading(mq_markdown::Heading { depth: mq_markdown::HeadingDepth::H1, values: vec![], position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into())],
        true
    )]
#[case::heading_depth_no_match(
        Node::Heading(mq_markdown::Heading { depth: mq_markdown::HeadingDepth::H2, values: vec![], position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
#[case::heading_multi_depth_match(
        Node::Heading(mq_markdown::Heading { depth: mq_markdown::HeadingDepth::H2, values: vec![], position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        true
    )]
#[case::heading_no_args_fallback(
        Node::Heading(mq_markdown::Heading { depth: mq_markdown::HeadingDepth::H1, values: vec![], position: None }),
        Selector::Heading(None),
        vec![],
        true
    )]
#[case::code_lang_match(
        Node::Code(mq_markdown::Code { lang: Some("rust".to_string()), meta: None, value: "fn main() {}".to_string(), fence: true, position: None }),
        Selector::Code,
        vec![RuntimeValue::String(Shared::new("rust".into()))],
        true
    )]
#[case::code_lang_no_match(
        Node::Code(mq_markdown::Code { lang: Some("python".to_string()), meta: None, value: "pass".to_string(), fence: true, position: None }),
        Selector::Code,
        vec![RuntimeValue::String(Shared::new("rust".into()))],
        false
    )]
#[case::code_no_args_fallback(
        Node::Code(mq_markdown::Code { lang: None, meta: None, value: "".to_string(), fence: true, position: None }),
        Selector::Code,
        vec![],
        true
    )]
#[case::callout_kind_match(
        Node::Callout(mq_markdown::Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![RuntimeValue::String(Shared::new("NOTE".into()))],
        true
    )]
#[case::callout_kind_no_match(
        Node::Callout(mq_markdown::Callout { fold: None, kind: "WARNING".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![RuntimeValue::String(Shared::new("NOTE".into()))],
        false
    )]
#[case::callout_multi_kind_match(
        Node::Callout(mq_markdown::Callout { fold: None, kind: "TIP".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![RuntimeValue::String(Shared::new("NOTE".into())), RuntimeValue::String(Shared::new("TIP".into()))],
        true
    )]
#[case::callout_no_args_fallback(
        Node::Callout(mq_markdown::Callout { fold: None, kind: "NOTE".to_string(), title: None, values: vec![], position: None }),
        Selector::Callout,
        vec![],
        true
    )]
#[case::callout_non_callout_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::Callout,
        vec![RuntimeValue::String(Shared::new("NOTE".into()))],
        false
    )]
#[case::wikilink_target_match(
        Node::WikiLink(mq_markdown::WikiLink { target: "Some Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String(Shared::new("Some Page".into()))],
        true
    )]
#[case::wikilink_target_no_match(
        Node::WikiLink(mq_markdown::WikiLink { target: "Other Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String(Shared::new("Some Page".into()))],
        false
    )]
#[case::wikilink_multi_target_match(
        Node::WikiLink(mq_markdown::WikiLink { target: "Other Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String(Shared::new("Some Page".into())), RuntimeValue::String(Shared::new("Other Page".into()))],
        true
    )]
#[case::wikilink_no_args_fallback(
        Node::WikiLink(mq_markdown::WikiLink { target: "Some Page".to_string(), text: None, position: None }),
        Selector::WikiLink,
        vec![],
        true
    )]
#[case::wikilink_non_wikilink_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::WikiLink,
        vec![RuntimeValue::String(Shared::new("Some Page".into()))],
        false
    )]
#[case::embed_target_match(
        Node::Embed(mq_markdown::Embed { target: "image.png".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![RuntimeValue::String(Shared::new("image.png".into()))],
        true
    )]
#[case::embed_target_no_match(
        Node::Embed(mq_markdown::Embed { target: "note.md".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![RuntimeValue::String(Shared::new("image.png".into()))],
        false
    )]
#[case::embed_multi_target_match(
        Node::Embed(mq_markdown::Embed { target: "note.md".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![RuntimeValue::String(Shared::new("image.png".into())), RuntimeValue::String(Shared::new("note.md".into()))],
        true
    )]
#[case::embed_no_args_fallback(
        Node::Embed(mq_markdown::Embed { target: "image.png".to_string(), display: None, position: None }),
        Selector::Embed,
        vec![],
        true
    )]
#[case::embed_non_embed_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::Embed,
        vec![RuntimeValue::String(Shared::new("image.png".into()))],
        false
    )]
#[case::link_ref_ident_match(
        Node::LinkRef(mq_markdown::LinkRef { ident: "ref".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        true
    )]
#[case::link_ref_ident_no_match(
        Node::LinkRef(mq_markdown::LinkRef { ident: "other".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        false
    )]
#[case::link_ref_multi_ident_match(
        Node::LinkRef(mq_markdown::LinkRef { ident: "other".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String(Shared::new("ref".into())), RuntimeValue::String(Shared::new("other".into()))],
        true
    )]
#[case::link_ref_no_args_fallback(
        Node::LinkRef(mq_markdown::LinkRef { ident: "ref".to_string(), label: None, values: vec![], position: None }),
        Selector::LinkRef,
        vec![],
        true
    )]
#[case::link_ref_non_link_ref_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::LinkRef,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        false
    )]
#[case::image_ref_ident_match(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "ref".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        true
    )]
#[case::image_ref_ident_no_match(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "other".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        false
    )]
#[case::image_ref_multi_ident_match(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "other".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String(Shared::new("ref".into())), RuntimeValue::String(Shared::new("other".into()))],
        true
    )]
#[case::image_ref_no_args_fallback(
        Node::ImageRef(mq_markdown::ImageRef { alt: "".to_string(), ident: "ref".to_string(), label: None, position: None }),
        Selector::ImageRef,
        vec![],
        true
    )]
#[case::image_ref_non_image_ref_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::ImageRef,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        false
    )]
#[case::footnote_ref_ident_match(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "1".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String(Shared::new("1".into()))],
        true
    )]
#[case::footnote_ref_ident_no_match(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "2".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String(Shared::new("1".into()))],
        false
    )]
#[case::footnote_ref_multi_ident_match(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "2".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String(Shared::new("1".into())), RuntimeValue::String(Shared::new("2".into()))],
        true
    )]
#[case::footnote_ref_no_args_fallback(
        Node::FootnoteRef(mq_markdown::FootnoteRef { ident: "1".to_string(), label: None, position: None }),
        Selector::FootnoteRef,
        vec![],
        true
    )]
#[case::footnote_ref_non_footnote_ref_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::FootnoteRef,
        vec![RuntimeValue::String(Shared::new("1".into()))],
        false
    )]
#[case::footnote_ident_match(
        Node::Footnote(mq_markdown::Footnote { ident: "1".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String(Shared::new("1".into()))],
        true
    )]
#[case::footnote_ident_no_match(
        Node::Footnote(mq_markdown::Footnote { ident: "2".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String(Shared::new("1".into()))],
        false
    )]
#[case::footnote_multi_ident_match(
        Node::Footnote(mq_markdown::Footnote { ident: "2".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String(Shared::new("1".into())), RuntimeValue::String(Shared::new("2".into()))],
        true
    )]
#[case::footnote_no_args_fallback(
        Node::Footnote(mq_markdown::Footnote { ident: "1".to_string(), values: vec![], position: None }),
        Selector::Footnote,
        vec![],
        true
    )]
#[case::footnote_non_footnote_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::Footnote,
        vec![RuntimeValue::String(Shared::new("1".into()))],
        false
    )]
#[case::definition_ident_match(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "ref".to_string(), label: None }),
        Selector::Definition,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        true
    )]
#[case::definition_ident_no_match(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "other".to_string(), label: None }),
        Selector::Definition,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        false
    )]
#[case::definition_multi_ident_match(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "other".to_string(), label: None }),
        Selector::Definition,
        vec![RuntimeValue::String(Shared::new("ref".into())), RuntimeValue::String(Shared::new("other".into()))],
        true
    )]
#[case::definition_no_args_fallback(
        Node::Definition(mq_markdown::Definition { position: None, url: mq_markdown::Url::new("https://example.com".to_string()), title: None, ident: "ref".to_string(), label: None }),
        Selector::Definition,
        vec![],
        true
    )]
#[case::definition_non_definition_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::Definition,
        vec![RuntimeValue::String(Shared::new("ref".into()))],
        false
    )]
#[case::mdx_jsx_flow_element_name_match(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("Alert".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String(Shared::new("Alert".into()))],
        true
    )]
#[case::mdx_jsx_flow_element_name_no_match(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String(Shared::new("Alert".into()))],
        false
    )]
#[case::mdx_jsx_flow_element_multi_name_match(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("div".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String(Shared::new("Alert".into())), RuntimeValue::String(Shared::new("div".into()))],
        true
    )]
#[case::mdx_jsx_flow_element_no_args_fallback(
        Node::MdxJsxFlowElement(mq_markdown::MdxJsxFlowElement { name: Some("Alert".to_string()), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxFlowElement,
        vec![],
        true
    )]
#[case::mdx_jsx_flow_element_non_matching_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::MdxJsxFlowElement,
        vec![RuntimeValue::String(Shared::new("Alert".into()))],
        false
    )]
#[case::mdx_jsx_text_element_name_match(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("Alert")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String(Shared::new("Alert".into()))],
        true
    )]
#[case::mdx_jsx_text_element_name_no_match(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("span")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String(Shared::new("Alert".into()))],
        false
    )]
#[case::mdx_jsx_text_element_multi_name_match(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("span")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String(Shared::new("Alert".into())), RuntimeValue::String(Shared::new("span".into()))],
        true
    )]
#[case::mdx_jsx_text_element_no_args_fallback(
        Node::MdxJsxTextElement(mq_markdown::MdxJsxTextElement { name: Some(SmolStr::new("Alert")), attributes: Vec::new(), children: Vec::new(), position: None }),
        Selector::MdxJsxTextElement,
        vec![],
        true
    )]
#[case::mdx_jsx_text_element_non_matching_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::MdxJsxTextElement,
        vec![RuntimeValue::String(Shared::new("Alert".into()))],
        false
    )]
#[case::non_heading_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::Heading(None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
#[case::list_index_match(
        Node::List(mq_markdown::List { marker: None, index: 2, level: 0, checked: None, ordered: false, start: None, spread: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(2.into())],
        true
    )]
#[case::list_index_no_match(
        Node::List(mq_markdown::List { marker: None, index: 0, level: 0, checked: None, ordered: false, start: None, spread: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
#[case::list_multi_index_match(
        Node::List(mq_markdown::List { marker: None, index: 3, level: 0, checked: None, ordered: false, start: None, spread: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(3.into())],
        true
    )]
#[case::list_negative_index_no_match(
        Node::List(mq_markdown::List { marker: None, index: 0, level: 0, checked: None, ordered: false, start: None, spread: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number((-1).into())],
        false
    )]
#[case::list_fractional_index_no_match(
        Node::List(mq_markdown::List { marker: None, index: 1, level: 0, checked: None, ordered: false, start: None, spread: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(1.5.into())],
        false
    )]
#[case::table_negative_row_no_match(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 0, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number((-1).into())],
        false
    )]
#[case::list_no_args_fallback(
        Node::List(mq_markdown::List { marker: None, index: 0, level: 0, checked: None, ordered: false, start: None, spread: false, values: vec![], position: None }),
        Selector::List(None, None),
        vec![],
        true
    )]
#[case::list_non_list_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::List(None, None),
        vec![RuntimeValue::Number(0.into())],
        false
    )]
#[case::table_row_match(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 1, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into())],
        true
    )]
#[case::table_row_no_match(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 0, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into())],
        false
    )]
#[case::table_row_and_col_match(
        Node::TableCell(mq_markdown::TableCell { column: 2, row: 1, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        true
    )]
#[case::table_row_and_col_no_match(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 1, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())],
        false
    )]
#[case::table_no_args_fallback(
        Node::TableCell(mq_markdown::TableCell { column: 0, row: 0, values: vec![], position: None }),
        Selector::Table(None, None),
        vec![],
        true
    )]
#[case::table_non_table_node(
        Node::HorizontalRule(mq_markdown::HorizontalRule { marker: None, position: None }),
        Selector::Table(None, None),
        vec![RuntimeValue::Number(0.into())],
        false
    )]
fn test_eval_selector_with_args(
    #[case] node: Node,
    #[case] selector: Selector,
    #[case] args: Vec<RuntimeValue>,
    #[case] expected_match: bool,
) {
    let result = eval_selector_with_args(&node, &selector, &args);
    assert_eq!(!result.is_none(), expected_match);
}

fn env() -> VmEnv {
    VmEnv::default()
}

fn call(name: &str, args: Vec<RuntimeValue>) -> Result<RuntimeValue, Error> {
    eval_builtin(&RuntimeValue::None, &Ident::new(name), args.into(), &env())
}

#[rstest]
#[case::get("get", "key_only_looked_up_by_get")]
#[case::has("has", "key_only_looked_up_by_has")]
#[case::del("del", "key_only_looked_up_by_del")]
fn test_dict_lookup_does_not_intern_missing_key(#[case] name: &str, #[case] key: &str) {
    let dict = RuntimeValue::Dict(Shared::new(DictMap::from_iter([(
        Ident::new("present"),
        RuntimeValue::Number(1.into()),
    )])));

    let result = call(name, vec![dict.clone(), key.into()]).unwrap();

    assert!(Ident::lookup(key).is_none());
    match name {
        "get" => assert_eq!(result, RuntimeValue::NONE),
        "has" => assert_eq!(result, RuntimeValue::Boolean(false)),
        _ => assert_eq!(result, dict),
    }
}

// =========================================================================
// band
// =========================================================================

#[rstest]
#[case(vec![0xff, 0xff], vec![0xff, 0xff], vec![0xff, 0xff])]
#[case(vec![0xf0, 0x0f], vec![0xff, 0xff], vec![0xf0, 0x0f])]
#[case(vec![0xaa, 0x55], vec![0x55, 0xaa], vec![0x00, 0x00])]
#[case(vec![0xff],       vec![0x00],       vec![0x00])]
#[case(vec![],           vec![],           vec![])]
fn test_band(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>, #[case] expected: Vec<u8>) {
    assert_eq!(
        call(
            "band",
            vec![RuntimeValue::Bytes(b1.into()), RuntimeValue::Bytes(b2.into())]
        ),
        Ok(RuntimeValue::Bytes(expected.into()))
    );
}

#[rstest]
#[case(vec![0x01, 0x02], vec![0x01])]
#[case(vec![],           vec![0x00])]
fn test_band_length_mismatch(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>) {
    assert!(
        call(
            "band",
            vec![RuntimeValue::Bytes(b1.into()), RuntimeValue::Bytes(b2.into())]
        )
        .is_err()
    );
}

#[test]
fn test_band_type_error() {
    assert!(
        call(
            "band",
            vec![
                RuntimeValue::String(Shared::new("a".into())),
                RuntimeValue::Bytes(Shared::new(vec![0x01]))
            ]
        )
        .is_err()
    );
}

// =========================================================================
// bor
// =========================================================================

#[rstest]
#[case(vec![0x00, 0x00], vec![0x00, 0x00], vec![0x00, 0x00])]
#[case(vec![0xf0, 0x00], vec![0x0f, 0x00], vec![0xff, 0x00])]
#[case(vec![0xaa, 0x55], vec![0x55, 0xaa], vec![0xff, 0xff])]
#[case(vec![0x00],       vec![0xff],       vec![0xff])]
#[case(vec![],           vec![],           vec![])]
fn test_bor(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>, #[case] expected: Vec<u8>) {
    assert_eq!(
        call(
            "bor",
            vec![RuntimeValue::Bytes(b1.into()), RuntimeValue::Bytes(b2.into())]
        ),
        Ok(RuntimeValue::Bytes(expected.into()))
    );
}

#[rstest]
#[case(vec![0x01, 0x02], vec![0x01])]
#[case(vec![],           vec![0x00])]
fn test_bor_length_mismatch(#[case] b1: Vec<u8>, #[case] b2: Vec<u8>) {
    assert!(
        call(
            "bor",
            vec![RuntimeValue::Bytes(b1.into()), RuntimeValue::Bytes(b2.into())]
        )
        .is_err()
    );
}

#[test]
fn test_bor_type_error() {
    assert!(
        call(
            "bor",
            vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Bytes(Shared::new(vec![0x01]))
            ]
        )
        .is_err()
    );
}

// =========================================================================
// bnot
// =========================================================================

#[rstest]
#[case(vec![0x00],       vec![0xff])]
#[case(vec![0xff],       vec![0x00])]
#[case(vec![0xf0, 0x0f], vec![0x0f, 0xf0])]
#[case(vec![0x55, 0xaa], vec![0xaa, 0x55])]
#[case(vec![],           vec![])]
fn test_bnot(#[case] input: Vec<u8>, #[case] expected: Vec<u8>) {
    assert_eq!(
        call("bnot", vec![RuntimeValue::Bytes(input.into())]),
        Ok(RuntimeValue::Bytes(expected.into()))
    );
}

#[test]
fn test_bnot_double_negation() {
    let original = vec![0xde, 0xad, 0xbe, 0xef];
    let once = call("bnot", vec![RuntimeValue::Bytes(Shared::new(original.clone()))]).unwrap();
    let twice = call("bnot", vec![once]).unwrap();
    assert_eq!(twice, RuntimeValue::Bytes(original.into()));
}

#[test]
fn test_bnot_type_error() {
    assert!(call("bnot", vec![RuntimeValue::String(Shared::new("a".into()))]).is_err());
}

// =========================================================================
// starts_with / ends_with for bytes
// =========================================================================

#[rstest]
#[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02],             true)]
#[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02, 0x03],       true)]
#[case(vec![0x01, 0x02, 0x03], vec![0x02, 0x03],             false)]
#[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02, 0x03, 0x04], false)]
#[case(vec![0x01],             vec![],                        true)]
#[case(vec![],                 vec![],                        true)]
fn test_bytes_starts_with(#[case] haystack: Vec<u8>, #[case] prefix: Vec<u8>, #[case] expected: bool) {
    assert_eq!(
        call(
            "starts_with",
            vec![RuntimeValue::Bytes(haystack.into()), RuntimeValue::Bytes(prefix.into())]
        ),
        Ok(RuntimeValue::Boolean(expected))
    );
}

#[rstest]
#[case(vec![0x01, 0x02, 0x03], vec![0x02, 0x03],             true)]
#[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02, 0x03],       true)]
#[case(vec![0x01, 0x02, 0x03], vec![0x01, 0x02],             false)]
#[case(vec![0x01, 0x02, 0x03], vec![0x00, 0x01, 0x02, 0x03], false)]
#[case(vec![0x01],             vec![],                        true)]
#[case(vec![],                 vec![],                        true)]
fn test_bytes_ends_with(#[case] haystack: Vec<u8>, #[case] suffix: Vec<u8>, #[case] expected: bool) {
    assert_eq!(
        call(
            "ends_with",
            vec![RuntimeValue::Bytes(haystack.into()), RuntimeValue::Bytes(suffix.into())]
        ),
        Ok(RuntimeValue::Boolean(expected))
    );
}

// =========================================================================
// index / rindex for bytes
// =========================================================================

#[rstest]
#[case(vec![0x01, 0x02, 0x03, 0x02],     vec![0x02],       1)]
#[case(vec![0x01, 0x02, 0x03],           vec![0x04],       -1)]
#[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02], 0)]
#[case(vec![0x01, 0x02, 0x03],           vec![0x02, 0x03], 1)]
#[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02, 0x03], 0)]
#[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02, 0x03, 0x04], -1)]
#[case(vec![],                           vec![0x01],       -1)]
fn test_bytes_index(#[case] haystack: Vec<u8>, #[case] needle: Vec<u8>, #[case] expected: i64) {
    assert_eq!(
        call(
            "index",
            vec![RuntimeValue::Bytes(haystack.into()), RuntimeValue::Bytes(needle.into())]
        ),
        Ok(RuntimeValue::Number(expected.into()))
    );
}

#[rstest]
#[case(vec![0x01, 0x02, 0x03, 0x02],     vec![0x02],       3)]
#[case(vec![0x01, 0x02, 0x03],           vec![0x04],       -1)]
#[case(vec![0x01, 0x02, 0x03, 0x01, 0x02], vec![0x01, 0x02], 3)]
#[case(vec![0x01, 0x02, 0x03],           vec![0x01, 0x02, 0x03], 0)]
#[case(vec![],                           vec![0x01],       -1)]
fn test_bytes_rindex(#[case] haystack: Vec<u8>, #[case] needle: Vec<u8>, #[case] expected: i64) {
    assert_eq!(
        call(
            "rindex",
            vec![RuntimeValue::Bytes(haystack.into()), RuntimeValue::Bytes(needle.into())]
        ),
        Ok(RuntimeValue::Number(expected.into()))
    );
}

#[test]
fn test_bytes_index_rindex_agree_single_occurrence() {
    let h = vec![0xaa, 0xbb, 0xcc];
    let n = vec![0xbb];
    let idx = call(
        "index",
        vec![
            RuntimeValue::Bytes(Shared::new(h.clone())),
            RuntimeValue::Bytes(Shared::new(n.clone())),
        ],
    )
    .unwrap();
    let ridx = call(
        "rindex",
        vec![RuntimeValue::Bytes(h.into()), RuntimeValue::Bytes(n.into())],
    )
    .unwrap();
    assert_eq!(idx, ridx);
}

// =========================================================================
// indices
// =========================================================================

#[rstest]
#[case::string_multiple("hello world hello", "hello", vec![0, 12])]
#[case::string_overlap_not_double_counted("aaaa", "aa", vec![0, 2])]
#[case::string_not_found("hello", "xyz", vec![])]
#[case::string_empty_needle("hello", "", vec![])]
fn test_string_indices(#[case] haystack: &str, #[case] needle: &str, #[case] expected: Vec<i64>) {
    assert_eq!(
        call(
            "indices",
            vec![
                RuntimeValue::String(Shared::new(haystack.to_string())),
                RuntimeValue::String(Shared::new(needle.to_string()))
            ]
        ),
        Ok(RuntimeValue::Array(Shared::new(
            expected.into_iter().map(|i| RuntimeValue::Number(i.into())).collect()
        )))
    );
}

#[rstest]
#[case(vec![0x01, 0x02, 0x03, 0x02], vec![0x02], vec![1, 3])]
#[case(vec![0x01, 0x02, 0x03],       vec![0x04], vec![])]
#[case(vec![0x01, 0x02, 0x01, 0x02], vec![0x01, 0x02], vec![0, 2])]
#[case(vec![],                      vec![0x01], vec![])]
#[case(vec![0x01],                  vec![],     vec![])]
fn test_bytes_indices(#[case] haystack: Vec<u8>, #[case] needle: Vec<u8>, #[case] expected: Vec<i64>) {
    assert_eq!(
        call(
            "indices",
            vec![RuntimeValue::Bytes(haystack.into()), RuntimeValue::Bytes(needle.into())]
        ),
        Ok(RuntimeValue::Array(Shared::new(
            expected.into_iter().map(|i| RuntimeValue::Number(i.into())).collect()
        )))
    );
}

#[test]
fn test_array_indices() {
    let array = RuntimeValue::Array(Shared::new(vec![
        RuntimeValue::String(Shared::new("a".to_string())),
        RuntimeValue::String(Shared::new("b".to_string())),
        RuntimeValue::String(Shared::new("a".to_string())),
    ]));
    assert_eq!(
        call(
            "indices",
            vec![array, RuntimeValue::String(Shared::new("a".to_string()))]
        ),
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::Number(0.into()),
            RuntimeValue::Number(2.into()),
        ])))
    );
}

#[test]
fn test_array_indices_not_found() {
    let array = RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("a".to_string()))]));
    assert_eq!(
        call(
            "indices",
            vec![array, RuntimeValue::String(Shared::new("z".to_string()))]
        ),
        Ok(RuntimeValue::Array(Shared::new(Vec::new())))
    );
}

// =========================================================================
// repeat for bytes
// =========================================================================

#[rstest]
#[case(vec![0x01, 0x02], 0, vec![])]
#[case(vec![0x01, 0x02], 1, vec![0x01, 0x02])]
#[case(vec![0x01, 0x02], 3, vec![0x01, 0x02, 0x01, 0x02, 0x01, 0x02])]
#[case(vec![0xff],       4, vec![0xff, 0xff, 0xff, 0xff])]
#[case(vec![],           5, vec![])]
fn test_bytes_repeat(#[case] input: Vec<u8>, #[case] n: u32, #[case] expected: Vec<u8>) {
    assert_eq!(
        call(
            "repeat",
            vec![
                RuntimeValue::Bytes(input.into()),
                RuntimeValue::Number((n as f64).into())
            ]
        ),
        Ok(RuntimeValue::Bytes(expected.into()))
    );
}

// =========================================================================
// pack
// =========================================================================

#[rstest]
#[case("u8",    0.0,    vec![0x00])]
#[case("u8",    255.0,  vec![0xff])]
#[case("i8",    -1.0,   vec![0xff])]
#[case("i8",    -128.0, vec![0x80])]
#[case("i8",    127.0,  vec![0x7f])]
#[case("u16be", 256.0,  vec![0x01, 0x00])]
#[case("u16le", 256.0,  vec![0x00, 0x01])]
#[case("i16be", -1.0,   vec![0xff, 0xff])]
#[case("i16le", -1.0,   vec![0xff, 0xff])]
#[case("u32be", 1.0,    vec![0x00, 0x00, 0x00, 0x01])]
#[case("u32le", 1.0,    vec![0x01, 0x00, 0x00, 0x00])]
#[case("i32be", -1.0,   vec![0xff, 0xff, 0xff, 0xff])]
#[case("i32le", -1.0,   vec![0xff, 0xff, 0xff, 0xff])]
#[case("u64be", 1.0,    vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01])]
#[case("u64le", 1.0,    vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
#[case("i64be", -1.0,   vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff])]
#[case("f32be", 1.0,    vec![0x3f, 0x80, 0x00, 0x00])]
#[case("f32le", 1.0,    vec![0x00, 0x00, 0x80, 0x3f])]
#[case("f64be", 1.0,    vec![0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
#[case("f64le", 1.0,    vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f])]
fn test_pack(#[case] fmt: &str, #[case] value: f64, #[case] expected: Vec<u8>) {
    assert_eq!(
        call(
            "pack",
            vec![
                RuntimeValue::String(Shared::new(fmt.into())),
                RuntimeValue::Number(value.into())
            ]
        ),
        Ok(RuntimeValue::Bytes(expected.into()))
    );
}

#[rstest]
#[case("z99")]
#[case("u16")]
#[case("")]
fn test_pack_unknown_format(#[case] fmt: &str) {
    assert!(
        call(
            "pack",
            vec![
                RuntimeValue::String(Shared::new(fmt.into())),
                RuntimeValue::Number(0.0.into())
            ]
        )
        .is_err()
    );
}

#[test]
fn test_pack_type_error() {
    assert!(
        call(
            "pack",
            vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(0.0.into())]
        )
        .is_err()
    );
}

// =========================================================================
// unpack
// =========================================================================

#[rstest]
#[case("u8",    vec![0x2a],                                               42.0)]
#[case("i8",    vec![0xff],                                               -1.0)]
#[case("u16be", vec![0x01, 0x00],                                         256.0)]
#[case("u16le", vec![0x00, 0x01],                                         256.0)]
#[case("i16be", vec![0xff, 0xff],                                         -1.0)]
#[case("i16le", vec![0xff, 0xff],                                         -1.0)]
#[case("u32be", vec![0x00, 0x00, 0x00, 0x01],                             1.0)]
#[case("u32le", vec![0x01, 0x00, 0x00, 0x00],                             1.0)]
#[case("i32be", vec![0xff, 0xff, 0xff, 0xff],                             -1.0)]
#[case("i32le", vec![0xff, 0xff, 0xff, 0xff],                             -1.0)]
#[case("u64be", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],    1.0)]
#[case("u64le", vec![0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],    1.0)]
#[case("i64be", vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],    -1.0)]
#[case("f32be", vec![0x3f, 0x80, 0x00, 0x00],                             1.0)]
#[case("f32le", vec![0x00, 0x00, 0x80, 0x3f],                             1.0)]
#[case("f64be", vec![0x3f, 0xf0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],    1.0)]
#[case("f64le", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xf0, 0x3f],    1.0)]
fn test_unpack(#[case] fmt: &str, #[case] bytes: Vec<u8>, #[case] expected: f64) {
    assert_eq!(
        call(
            "unpack",
            vec![
                RuntimeValue::String(Shared::new(fmt.into())),
                RuntimeValue::Bytes(bytes.into())
            ]
        ),
        Ok(RuntimeValue::Number(expected.into()))
    );
}

#[rstest]
#[case("u8",    vec![])]
#[case("u16be", vec![0x00])]
#[case("u32be", vec![0x00, 0x00, 0x00])]
#[case("u64be", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
#[case("f32be", vec![0x00, 0x00, 0x00])]
#[case("f64be", vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00])]
fn test_unpack_too_short(#[case] fmt: &str, #[case] bytes: Vec<u8>) {
    assert!(
        call(
            "unpack",
            vec![
                RuntimeValue::String(Shared::new(fmt.into())),
                RuntimeValue::Bytes(bytes.into())
            ]
        )
        .is_err()
    );
}

#[rstest]
#[case("z99")]
#[case("")]
fn test_unpack_unknown_format(#[case] fmt: &str) {
    assert!(
        call(
            "unpack",
            vec![
                RuntimeValue::String(Shared::new(fmt.into())),
                RuntimeValue::Bytes(Shared::new(vec![0x00]))
            ]
        )
        .is_err()
    );
}

#[test]
fn test_unpack_type_error() {
    assert!(
        call(
            "unpack",
            vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Bytes(Shared::new(vec![0x00]))
            ]
        )
        .is_err()
    );
}

#[rstest]
#[case("u8", 42.0)]
#[case("i8",    -5.0)]
#[case("u16be", 1234.0)]
#[case("u16le", 1234.0)]
#[case("i16be", -1000.0)]
#[case("i16le", -1000.0)]
#[case("u32be", 100000.0)]
#[case("u32le", 100000.0)]
#[case("i32be", -100000.0)]
#[case("i32le", -100000.0)]
#[case("u64be", 1000000.0)]
#[case("u64le", 1000000.0)]
#[case("i64be", -1000000.0)]
#[case("i64le", -1000000.0)]
#[case("f32be", 1.5)]
#[case("f32le", 1.5)]
#[case("f64be", 1.23456789)]
#[case("f64le", 1.23456789)]
fn test_pack_unpack_roundtrip(#[case] fmt: &str, #[case] value: f64) {
    let packed = call(
        "pack",
        vec![
            RuntimeValue::String(Shared::new(fmt.into())),
            RuntimeValue::Number(value.into()),
        ],
    )
    .unwrap();
    let result = call("unpack", vec![RuntimeValue::String(Shared::new(fmt.into())), packed]).unwrap();
    match result {
        RuntimeValue::Number(n) => assert!((n.value() - value).abs() < 1e-5),
        _ => panic!("expected Number"),
    }
}

#[cfg(feature = "file-io")]
#[test]
fn test_open_file_capability_gate_and_incremental_reads() {
    use std::io::Write;

    let mut tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    tmp.write_all(b"one\r\ntwo\nthree").expect("failed to write");
    let path = RuntimeValue::String(Shared::new(tmp.path().to_string_lossy().to_string()));
    let string = |s: &str| RuntimeValue::String(Shared::new(s.to_string()));

    assert!(
        call("open_file", vec![path.clone()]).is_err(),
        "open_file should be blocked when read access is not allowed"
    );

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));
    let handle = call("open_file", vec![path.clone()]).expect("open_file should succeed");
    assert_eq!(call("type", vec![handle.clone()]), Ok(string("file")));

    assert_eq!(call("read_line", vec![handle.clone()]), Ok(string("one")));
    assert_eq!(
        call("read_bytes", vec![handle.clone(), RuntimeValue::Number(2.into())]),
        Ok(RuntimeValue::Bytes(Shared::new(b"tw".to_vec())))
    );
    assert_eq!(call("read_line", vec![handle.clone()]), Ok(string("o")));
    assert_eq!(call("read_line", vec![handle.clone()]), Ok(string("three")));
    assert_eq!(call("read_line", vec![handle.clone()]), Ok(RuntimeValue::NONE));
    assert_eq!(
        call("read_bytes", vec![handle.clone(), RuntimeValue::Number(1.into())]),
        Ok(RuntimeValue::NONE)
    );

    assert_eq!(
        call("status", vec![handle.clone()]),
        Ok(RuntimeValue::Symbol(Ident::new("open")))
    );
    assert_eq!(call("close", vec![handle.clone()]), Ok(handle.clone()));
    assert_eq!(
        call("status", vec![handle.clone()]),
        Ok(RuntimeValue::Symbol(Ident::new("closed")))
    );
    assert!(call("read_line", vec![handle.clone()]).is_err());

    let open = call("open_file", vec![path.clone()]).unwrap();
    for size in [0.0, 1e-20, -1.0, 1.5, 1e300, (MAX_READ_BYTES + 1) as f64] {
        assert!(
            call("read_bytes", vec![open.clone(), RuntimeValue::Number(size.into())]).is_err(),
            "read_bytes should reject size {size}"
        );
    }
    assert!(call("read_line", vec![path.clone()]).is_err());
    assert!(call("open_file", vec![RuntimeValue::Number(42.into())]).is_err());
    assert!(
        call(
            "open_file",
            vec![RuntimeValue::String(Shared::new(
                "/nonexistent/path/no_such_file.txt".into()
            ))]
        )
        .is_err(),
        "open_file should error for a nonexistent file"
    );
}

#[cfg(feature = "file-io")]
#[test]
fn test_read_capability_gate_and_success() {
    use std::io::Write;

    let get = |entry: &RuntimeValue, key: &str| match entry {
        RuntimeValue::Dict(d) => d.get(&Ident::new(key)).cloned().unwrap_or(RuntimeValue::NONE),
        other => panic!("expected Dict, got {other:?}"),
    };
    let as_entries = |result: RuntimeValue| match result {
        RuntimeValue::Array(entries) => entries,
        other => panic!("expected Array, got {other:?}"),
    };

    let mut text_tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    text_tmp.write_all(b"hello").expect("failed to write");
    let text_path = text_tmp.path().to_string_lossy().to_string();

    let mut bytes_tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    bytes_tmp.write_all(&[0x89, 0x50, 0x4e, 0x47]).expect("failed to write");
    let bytes_path = bytes_tmp.path().to_string_lossy().to_string();

    let collection_dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(collection_dir.path().join("root.md"), "# Root\n").expect("failed to write");

    // No ambient Io guard installed yet: the default is all-denied.
    assert!(
        call("read_file", vec![RuntimeValue::String(Shared::new(text_path.clone()))]).is_err(),
        "read_file should be blocked when read access is not allowed"
    );
    assert!(
        call(
            "read_file_bytes",
            vec![RuntimeValue::String(Shared::new(bytes_path.clone()))]
        )
        .is_err(),
        "read_file_bytes should be blocked when read access is not allowed"
    );
    assert!(
        call(
            "collection",
            vec![RuntimeValue::String(Shared::new(
                collection_dir.path().to_string_lossy().into_owned()
            ))],
        )
        .is_err(),
        "collection should be blocked when read access is not allowed"
    );
    assert!(
        call(
            "file_exists",
            vec![RuntimeValue::String(Shared::new(text_path.clone()))]
        )
        .is_err(),
        "file_exists should be blocked when read access is not allowed"
    );
    assert!(
        call("file_size", vec![RuntimeValue::String(Shared::new(text_path.clone()))]).is_err(),
        "file_size should be blocked when read access is not allowed"
    );
    assert!(
        call("file_info", vec![RuntimeValue::String(Shared::new(text_path.clone()))]).is_err(),
        "file_info should be blocked when read access is not allowed"
    );

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));
    assert_eq!(
        call("read_file", vec![RuntimeValue::String(Shared::new(text_path.clone()))]),
        Ok(RuntimeValue::String(Shared::new("hello".to_string())))
    );
    assert_eq!(
        call(
            "read_file_bytes",
            vec![RuntimeValue::String(Shared::new(bytes_path.clone()))]
        ),
        Ok(RuntimeValue::Bytes(Shared::new(vec![0x89, 0x50, 0x4e, 0x47])))
    );

    let result = call(
        "read_file_bytes",
        vec![RuntimeValue::String(Shared::new(
            "/nonexistent/path/no_such_file.png".into(),
        ))],
    );
    assert!(result.is_err(), "read_file_bytes should error for a nonexistent file");

    let result = call(
        "read_file",
        vec![RuntimeValue::String(Shared::new(
            "/nonexistent/path/no_such_file.md".into(),
        ))],
    );
    assert!(result.is_err(), "read_file should error for a nonexistent file");

    assert_eq!(
        call(
            "file_exists",
            vec![RuntimeValue::String(Shared::new(text_path.clone()))]
        ),
        Ok(RuntimeValue::Boolean(true))
    );
    assert_eq!(
        call(
            "file_exists",
            vec![RuntimeValue::String(Shared::new(
                "/nonexistent/path/no_such_file.md".into()
            ))]
        ),
        Ok(RuntimeValue::Boolean(false))
    );
    assert!(call("file_exists", vec![RuntimeValue::Number(42.into())]).is_err());

    assert_eq!(
        call("file_size", vec![RuntimeValue::String(Shared::new(text_path.clone()))]),
        Ok(RuntimeValue::Number(5.into()))
    );
    assert!(
        call(
            "file_size",
            vec![RuntimeValue::String(Shared::new(
                "/nonexistent/path/no_such_file.md".into()
            ))]
        )
        .is_err(),
        "file_size should error for a nonexistent file"
    );
    assert!(call("file_size", vec![RuntimeValue::Number(42.into())]).is_err());

    match call("file_info", vec![RuntimeValue::String(Shared::new(text_path.clone()))])
        .expect("file_info should succeed")
    {
        RuntimeValue::Dict(d) => {
            assert_eq!(
                d.get(&Ident::new("path")),
                Some(&RuntimeValue::String(Shared::new(text_path.clone())))
            );
            assert_eq!(
                d.get(&Ident::new("kind")),
                Some(&RuntimeValue::String(Shared::new("file".to_string())))
            );
            assert_eq!(d.get(&Ident::new("size")), Some(&RuntimeValue::Number(5.into())));
            assert!(matches!(d.get(&Ident::new("modified")), Some(RuntimeValue::Number(_))));
        }
        other => panic!("expected Dict, got {other:?}"),
    }
    assert!(
        call(
            "file_info",
            vec![RuntimeValue::String(Shared::new(
                "/nonexistent/path/no_such_file.md".into()
            ))]
        )
        .is_err(),
        "file_info should error for a nonexistent file"
    );
    assert!(call("file_info", vec![RuntimeValue::Number(42.into())]).is_err());

    #[cfg(unix)]
    {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        match call(
            "file_info",
            vec![RuntimeValue::String(Shared::new(
                dir.path().to_string_lossy().into_owned(),
            ))],
        )
        .expect("file_info should succeed for a directory")
        {
            RuntimeValue::Dict(d) => {
                assert_eq!(
                    d.get(&Ident::new("kind")),
                    Some(&RuntimeValue::String(Shared::new("dir".to_string())))
                );
            }
            other => panic!("expected Dict, got {other:?}"),
        }

        let target = dir.path().join("target.txt");
        std::fs::write(&target, "hello world").expect("failed to write");
        let link = dir.path().join("link.txt");
        std::os::unix::fs::symlink(&target, &link).expect("failed to create symlink");
        match call(
            "file_info",
            vec![RuntimeValue::String(Shared::new(link.to_string_lossy().into_owned()))],
        )
        .expect("file_info should succeed for a symlink")
        {
            RuntimeValue::Dict(d) => {
                assert_eq!(
                    d.get(&Ident::new("kind")),
                    Some(&RuntimeValue::String(Shared::new("symlink".to_string())))
                );
            }
            other => panic!("expected Dict, got {other:?}"),
        }

        // A cyclic symlink resolves instantly since file_info never follows symlinks.
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::os::unix::fs::symlink(&b, &a).expect("failed to create symlink");
        std::os::unix::fs::symlink(&a, &b).expect("failed to create symlink");
        match call(
            "file_info",
            vec![RuntimeValue::String(Shared::new(a.to_string_lossy().into_owned()))],
        )
        .expect("file_info should succeed for a cyclic symlink")
        {
            RuntimeValue::Dict(d) => {
                assert_eq!(
                    d.get(&Ident::new("kind")),
                    Some(&RuntimeValue::String(Shared::new("symlink".to_string())))
                );
            }
            other => panic!("expected Dict, got {other:?}"),
        }
    }

    // Basic collection: YAML/TOML frontmatter, title, content, sorted by path.
    {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(
            dir.path().join("b.md"),
            "---\ntitle: Hello\ntags:\n  - rust\n---\n\n# Hello\n\nbody text\n",
        )
        .expect("failed to write");
        std::fs::write(dir.path().join("a.md"), "+++\ntitle = \"World\"\n+++\n\n# World\n").expect("failed to write");
        std::fs::write(dir.path().join("c.md"), "# No frontmatter\n\nplain\n").expect("failed to write");
        std::fs::write(dir.path().join("ignore.txt"), "not markdown").expect("failed to write");

        let entries = as_entries(
            call(
                "collection",
                vec![RuntimeValue::String(Shared::new(
                    dir.path().to_string_lossy().into_owned(),
                ))],
            )
            .expect("collection should succeed"),
        );
        assert_eq!(entries.len(), 3);

        // sorted by path: a.md, b.md, c.md
        assert_eq!(
            get(&entries[0], "title"),
            RuntimeValue::String(Shared::new("World".into()))
        );
        let mut toml_frontmatter = DictMap::default();
        toml_frontmatter.insert(Ident::new("title"), RuntimeValue::String(Shared::new("World".into())));
        assert_eq!(
            get(&entries[0], "frontmatter"),
            RuntimeValue::Dict(Shared::new(toml_frontmatter))
        );

        assert_eq!(
            get(&entries[1], "title"),
            RuntimeValue::String(Shared::new("Hello".into()))
        );
        match get(&entries[1], "frontmatter") {
            RuntimeValue::Dict(d) => {
                assert_eq!(
                    d.get(&Ident::new("title")),
                    Some(&RuntimeValue::String(Shared::new("Hello".into())))
                );
                assert_eq!(
                    d.get(&Ident::new("tags")),
                    Some(&RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(
                        Shared::new("rust".into())
                    )])))
                );
            }
            other => panic!("expected Dict, got {other:?}"),
        }
        match get(&entries[1], "content") {
            RuntimeValue::Array(nodes) => {
                assert!(nodes.iter().any(|n| match n {
                    RuntimeValue::Markdown(node, _) => node.value().contains("body text"),
                    _ => false,
                }));
                assert!(
                    !nodes
                        .iter()
                        .any(|n| matches!(n, RuntimeValue::Markdown(node, _) if node.is_yaml()))
                );
            }
            other => panic!("expected Array, got {other:?}"),
        }

        assert_eq!(
            get(&entries[2], "title"),
            RuntimeValue::String(Shared::new("No frontmatter".into()))
        );
        assert_eq!(get(&entries[2], "frontmatter"), RuntimeValue::NONE);
    }

    // Recurses into subdirectories.
    {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");

        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).expect("failed to create subdir");
        std::fs::write(sub.join("nested.md"), "# Nested\n").expect("failed to write");

        let nested_sub = sub.join("deeper");
        std::fs::create_dir(&nested_sub).expect("failed to create nested subdir");
        std::fs::write(nested_sub.join("deepest.md"), "# Deepest\n").expect("failed to write");

        let entries = as_entries(
            call(
                "collection",
                vec![RuntimeValue::String(Shared::new(
                    dir.path().to_string_lossy().into_owned(),
                ))],
            )
            .expect("collection should succeed"),
        );
        assert_eq!(entries.len(), 3);

        let titles: Vec<_> = entries
            .iter()
            .map(|entry| match get(entry, "title") {
                RuntimeValue::String(s) => s,
                other => panic!("expected String, got {other:?}"),
            })
            .collect();
        assert!(titles.iter().any(|t| **t == "Root"));
        assert!(titles.iter().any(|t| **t == "Nested"));
        assert!(titles.iter().any(|t| **t == "Deepest"));
    }

    #[cfg(unix)]
    {
        // Follows symlinks to both files and directories.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(dir.path().join("real.md"), "# Real\n").expect("failed to write");

            let real_sub = dir.path().join("real_sub");
            std::fs::create_dir(&real_sub).expect("failed to create subdir");
            std::fs::write(real_sub.join("inside.md"), "# Inside\n").expect("failed to write");

            // Symlink to a file, placed directly in the root directory.
            std::os::unix::fs::symlink(dir.path().join("real.md"), dir.path().join("linked.md"))
                .expect("failed to create file symlink");

            // Symlink to a directory, which should be traversed like a normal directory.
            std::os::unix::fs::symlink(&real_sub, dir.path().join("linked_dir")).expect("failed to create dir symlink");

            let entries = as_entries(
                call(
                    "collection",
                    vec![RuntimeValue::String(Shared::new(
                        dir.path().to_string_lossy().into_owned(),
                    ))],
                )
                .expect("collection should succeed"),
            );
            let titles: Vec<_> = entries
                .iter()
                .map(|entry| match get(entry, "title") {
                    RuntimeValue::String(s) => s,
                    other => panic!("expected String, got {other:?}"),
                })
                .collect();

            // real.md, linked.md (-> real.md), real_sub/inside.md, linked_dir/inside.md (-> real_sub/inside.md)
            assert_eq!(entries.len(), 4);
            assert_eq!(titles.iter().filter(|t| ***t == "Real").count(), 2);
            assert_eq!(titles.iter().filter(|t| ***t == "Inside").count(), 2);
        }

        // Detects and stops at symlink cycles.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");

            let sub = dir.path().join("sub");
            std::fs::create_dir(&sub).expect("failed to create subdir");

            // Symlink back to the root directory, creating a cycle.
            std::os::unix::fs::symlink(dir.path(), sub.join("back_to_root")).expect("failed to create dir symlink");

            let entries = as_entries(
                call(
                    "collection",
                    vec![RuntimeValue::String(Shared::new(
                        dir.path().to_string_lossy().into_owned(),
                    ))],
                )
                .expect("collection should succeed despite the symlink cycle"),
            );
            assert_eq!(entries.len(), 1);
        }

        // Skips broken symlinks instead of erroring.
        {
            let dir = tempfile::tempdir().expect("failed to create temp dir");
            std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");

            std::os::unix::fs::symlink(dir.path().join("does_not_exist.md"), dir.path().join("broken.md"))
                .expect("failed to create broken symlink");

            let entries = as_entries(
                call(
                    "collection",
                    vec![RuntimeValue::String(Shared::new(
                        dir.path().to_string_lossy().into_owned(),
                    ))],
                )
                .expect("collection should succeed despite the broken symlink"),
            );
            assert_eq!(entries.len(), 1);
        }
    }

    // Skips empty subdirectories.
    {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(dir.path().join("root.md"), "# Root\n").expect("failed to write");
        std::fs::create_dir(dir.path().join("empty_sub")).expect("failed to create empty subdir");

        let entries = as_entries(
            call(
                "collection",
                vec![RuntimeValue::String(Shared::new(
                    dir.path().to_string_lossy().into_owned(),
                ))],
            )
            .expect("collection should succeed with an empty subdirectory present"),
        );
        assert_eq!(entries.len(), 1);
    }

    // Errors when the path is a file rather than a directory.
    {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let file_path = dir.path().join("not_a_dir.md");
        std::fs::write(&file_path, "# Root\n").expect("failed to write");

        let result = call(
            "collection",
            vec![RuntimeValue::String(Shared::new(
                file_path.to_string_lossy().into_owned(),
            ))],
        );
        assert!(result.is_err());
    }

    // Errors for a nonexistent directory.
    assert!(
        call(
            "collection",
            vec![RuntimeValue::String(Shared::new(
                "/nonexistent/path/no_such_dir".into()
            ))],
        )
        .is_err()
    );

    // Errors for an invalid argument type.
    assert!(call("collection", vec![RuntimeValue::Number(42.into())]).is_err());

    // respect_gitignore defaults to false: prior behavior is unchanged.
    {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(dir.path().join(".gitignore"), "ignored.md\n").expect("failed to write");
        std::fs::write(dir.path().join("ignored.md"), "# Ignored\n").expect("failed to write");
        std::fs::write(dir.path().join("kept.md"), "# Kept\n").expect("failed to write");

        let hidden_dir = dir.path().join(".hidden");
        std::fs::create_dir(&hidden_dir).expect("failed to create hidden dir");
        std::fs::write(hidden_dir.join("secret.md"), "# Secret\n").expect("failed to write");

        let entries = as_entries(
            call(
                "collection",
                vec![RuntimeValue::String(Shared::new(
                    dir.path().to_string_lossy().into_owned(),
                ))],
            )
            .expect("collection should succeed"),
        );
        assert_eq!(
            entries.len(),
            3,
            "ignored.md and .hidden/secret.md should still be collected"
        );
    }

    // respect_gitignore = true: skips dotfiles and .gitignore matches; nested .gitignore wins.
    {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        std::fs::write(dir.path().join(".gitignore"), "*.log.md\nbuild/\n").expect("failed to write");
        std::fs::write(dir.path().join("kept.md"), "# Kept\n").expect("failed to write");
        std::fs::write(dir.path().join("debug.log.md"), "# Debug\n").expect("failed to write");

        let hidden_dir = dir.path().join(".hidden");
        std::fs::create_dir(&hidden_dir).expect("failed to create hidden dir");
        std::fs::write(hidden_dir.join("secret.md"), "# Secret\n").expect("failed to write");

        let build_dir = dir.path().join("build");
        std::fs::create_dir(&build_dir).expect("failed to create build dir");
        std::fs::write(build_dir.join("out.md"), "# Out\n").expect("failed to write");

        // A subdirectory's .gitignore re-allows a file the parent's .gitignore ignores.
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).expect("failed to create subdir");
        std::fs::write(sub.join(".gitignore"), "!debug.log.md\n").expect("failed to write");
        std::fs::write(sub.join("debug.log.md"), "# Sub debug\n").expect("failed to write");

        let entries = as_entries(
            call(
                "collection",
                vec![
                    RuntimeValue::String(Shared::new(dir.path().to_string_lossy().into_owned())),
                    RuntimeValue::Boolean(true),
                ],
            )
            .expect("collection should succeed"),
        );
        let titles: Vec<_> = entries
            .iter()
            .map(|entry| match get(entry, "title") {
                RuntimeValue::String(s) => s,
                other => panic!("expected String, got {other:?}"),
            })
            .collect();

        assert_eq!(entries.len(), 2, "titles: {titles:?}");
        assert!(titles.iter().any(|t| **t == "Kept"));
        assert!(titles.iter().any(|t| **t == "Sub debug"));
    }

    // Errors for an invalid respect_gitignore argument type.
    assert!(
        call(
            "collection",
            vec![
                RuntimeValue::String(Shared::new("/nonexistent/path".into())),
                RuntimeValue::Number(1.into())
            ],
        )
        .is_err()
    );
}

#[cfg(feature = "file-io")]
fn walk_files_paths(result: RuntimeValue) -> Vec<String> {
    match result {
        RuntimeValue::Array(entries) => entries
            .iter()
            .map(|e| match e {
                RuntimeValue::String(s) => s.to_string(),
                other => panic!("expected String, got {other:?}"),
            })
            .collect(),
        other => panic!("expected Array, got {other:?}"),
    }
}

#[cfg(feature = "file-io")]
fn walk_files_options(entries: &[(&str, bool)]) -> RuntimeValue {
    let mut map = DictMap::default();
    for (key, value) in entries {
        map.insert(Ident::new(key), RuntimeValue::Boolean(*value));
    }
    RuntimeValue::Dict(Shared::new(map))
}

#[cfg(feature = "file-io")]
#[test]
fn test_walk_files_requires_allow_read() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("a.md"), "a").expect("failed to write");

    // No ambient Io guard installed: the default is all-denied.
    assert!(
        call(
            "walk_files",
            vec![RuntimeValue::String(Shared::new(
                dir.path().to_string_lossy().into_owned()
            ))],
        )
        .is_err(),
        "walk_files should be blocked when read access is not allowed"
    );
}

#[cfg(feature = "file-io")]
#[test]
fn test_walk_files_enumerates_all_file_types_sorted() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("b.txt"), "b").expect("failed to write");
    std::fs::write(dir.path().join("a.md"), "a").expect("failed to write");
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).expect("failed to create subdir");
    std::fs::write(sub.join("c.png"), "c").expect("failed to write");

    let paths = walk_files_paths(
        call(
            "walk_files",
            vec![RuntimeValue::String(Shared::new(
                dir.path().to_string_lossy().into_owned(),
            ))],
        )
        .expect("walk_files should succeed"),
    );
    assert_eq!(paths, vec!["a.md", "b.txt", "sub/c.png"]);
}

#[cfg(feature = "file-io")]
#[rstest]
// Default pattern ("**") matches every non-directory entry, Markdown or not.
#[case("**", vec!["a.md", "b.txt", "sub/c.md"])]
// A narrower glob restricts matches to a root-relative subset.
#[case("**/*.md", vec!["a.md", "sub/c.md"])]
fn test_walk_files_pattern(#[case] pattern: &str, #[case] expected: Vec<&str>) {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("a.md"), "a").expect("failed to write");
    std::fs::write(dir.path().join("b.txt"), "b").expect("failed to write");
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).expect("failed to create subdir");
    std::fs::write(sub.join("c.md"), "c").expect("failed to write");

    let paths = walk_files_paths(
        call(
            "walk_files",
            vec![
                RuntimeValue::String(Shared::new(dir.path().to_string_lossy().into_owned())),
                RuntimeValue::String(Shared::new(pattern.into())),
            ],
        )
        .expect("walk_files should succeed"),
    );
    assert_eq!(paths, expected);
}

#[cfg(feature = "file-io")]
#[rstest]
// Defaults to false: dotfiles/.gitignore matches are kept, same as `collection`.
#[case(None, vec![".gitignore", ".hidden/secret.txt", "ignored.txt", "kept.txt"])]
// true: skips dotfiles/dot-directories and any path a .gitignore matches.
#[case(Some(true), vec!["kept.txt"])]
fn test_walk_files_respect_gitignore(#[case] respect_gitignore: Option<bool>, #[case] expected: Vec<&str>) {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join(".gitignore"), "ignored.txt\n").expect("failed to write");
    std::fs::write(dir.path().join("ignored.txt"), "x").expect("failed to write");
    std::fs::write(dir.path().join("kept.txt"), "x").expect("failed to write");
    let hidden_dir = dir.path().join(".hidden");
    std::fs::create_dir(&hidden_dir).expect("failed to create hidden dir");
    std::fs::write(hidden_dir.join("secret.txt"), "x").expect("failed to write");

    let mut args = vec![RuntimeValue::String(Shared::new(
        dir.path().to_string_lossy().into_owned(),
    ))];
    if let Some(respect_gitignore) = respect_gitignore {
        args.push(RuntimeValue::String(Shared::new("**".into())));
        args.push(walk_files_options(&[("respect_gitignore", respect_gitignore)]));
    }

    let paths = walk_files_paths(call("walk_files", args).expect("walk_files should succeed"));
    assert_eq!(paths, expected);
}

#[cfg(all(feature = "file-io", unix))]
#[rstest]
// Defaults to false: a symlinked file is still included, but a symlinked
// directory is not descended into (so it contributes no entries at all).
#[case(None, vec!["linked.txt", "real.txt", "real_sub/inside.txt"])]
// true: descends into symlinked directories too.
#[case(
        Some(true),
        vec!["linked.txt", "linked_dir/inside.txt", "real.txt", "real_sub/inside.txt"]
    )]
fn test_walk_files_follow_symlinks(#[case] follow_symlinks: Option<bool>, #[case] expected: Vec<&str>) {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("real.txt"), "x").expect("failed to write");
    let real_sub = dir.path().join("real_sub");
    std::fs::create_dir(&real_sub).expect("failed to create subdir");
    std::fs::write(real_sub.join("inside.txt"), "x").expect("failed to write");
    std::os::unix::fs::symlink(dir.path().join("real.txt"), dir.path().join("linked.txt"))
        .expect("failed to create file symlink");
    std::os::unix::fs::symlink(&real_sub, dir.path().join("linked_dir")).expect("failed to create dir symlink");

    let mut args = vec![RuntimeValue::String(Shared::new(
        dir.path().to_string_lossy().into_owned(),
    ))];
    if let Some(follow_symlinks) = follow_symlinks {
        args.push(RuntimeValue::String(Shared::new("**".into())));
        args.push(walk_files_options(&[("follow_symlinks", follow_symlinks)]));
    }

    let paths = walk_files_paths(call("walk_files", args).expect("walk_files should succeed"));
    assert_eq!(paths, expected);
}

#[cfg(all(feature = "file-io", unix))]
#[test]
fn test_walk_files_detects_symlink_cycles_even_with_follow_symlinks() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("root.txt"), "x").expect("failed to write");
    let sub = dir.path().join("sub");
    std::fs::create_dir(&sub).expect("failed to create subdir");
    std::os::unix::fs::symlink(dir.path(), sub.join("back_to_root")).expect("failed to create dir symlink");

    let paths = walk_files_paths(
        call(
            "walk_files",
            vec![
                RuntimeValue::String(Shared::new(dir.path().to_string_lossy().into_owned())),
                RuntimeValue::String(Shared::new("**".into())),
                walk_files_options(&[("follow_symlinks", true)]),
            ],
        )
        .expect("walk_files should succeed despite the symlink cycle"),
    );
    assert_eq!(paths, vec!["root.txt"]);
}

#[cfg(all(feature = "file-io", unix))]
#[test]
fn test_walk_files_skips_broken_symlinks() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("root.txt"), "x").expect("failed to write");
    std::os::unix::fs::symlink(dir.path().join("does_not_exist"), dir.path().join("broken"))
        .expect("failed to create broken symlink");

    let paths = walk_files_paths(
        call(
            "walk_files",
            vec![RuntimeValue::String(Shared::new(
                dir.path().to_string_lossy().into_owned(),
            ))],
        )
        .expect("walk_files should succeed despite the broken symlink"),
    );
    assert_eq!(paths, vec!["root.txt"]);
}

// macOS/APFS rejects non-UTF-8 filenames outright, so this is only exercisable on Linux.
#[cfg(all(feature = "file-io", target_os = "linux"))]
#[test]
fn test_walk_files_includes_non_utf8_filenames() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let name = OsStr::from_bytes(b"report\xff.md");
    std::fs::write(dir.path().join(name), "x").expect("failed to write");

    let paths = walk_files_paths(
        call(
            "walk_files",
            vec![RuntimeValue::String(Shared::new(
                dir.path().to_string_lossy().into_owned(),
            ))],
        )
        .expect("walk_files should succeed"),
    );
    assert_eq!(paths.len(), 1, "non-UTF-8 filename should not be dropped");
}

#[cfg(all(feature = "file-io", unix))]
#[test]
fn test_walk_files_rejects_symlink_escape_outside_allowed_paths() {
    let allowed = tempfile::tempdir().expect("failed to create temp dir");
    let outside = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(outside.path().join("secret.md"), "secret").expect("failed to write");
    std::os::unix::fs::symlink(outside.path(), allowed.path().join("escape")).expect("failed to create symlink");

    let _guard = io_context::scoped(Shared::new(
        SandboxedIo::new(NativeIo::default()).allow_read(vec![allowed.path().to_path_buf()]),
    ));

    assert!(
        call(
            "walk_files",
            vec![
                RuntimeValue::String(Shared::new(allowed.path().to_string_lossy().into_owned())),
                RuntimeValue::String(Shared::new("**".into())),
                walk_files_options(&[("follow_symlinks", true)]),
            ],
        )
        .is_err(),
        "walking into a symlink that resolves outside the allowed paths must be denied"
    );

    // Calling walk_files directly on the escaping symlink is denied the same way.
    assert!(
        call(
            "walk_files",
            vec![RuntimeValue::String(Shared::new(
                allowed.path().join("escape").to_string_lossy().into_owned(),
            ))],
        )
        .is_err()
    );
}

#[cfg(feature = "file-io")]
#[rstest]
// Invalid root argument type.
#[case(|_dir: &str| vec![RuntimeValue::Number(42.into())])]
// Nonexistent directory.
#[case(|_dir: &str| vec![RuntimeValue::String(Shared::new("/nonexistent/path/no_such_dir".into()))])]
// Invalid glob pattern syntax.
#[case(|dir: &str| vec![
        RuntimeValue::String(Shared::new(dir.to_string())),
        RuntimeValue::String(Shared::new("[".into())),
    ])]
// Invalid pattern argument type.
#[case(|dir: &str| vec![
        RuntimeValue::String(Shared::new(dir.to_string())),
        RuntimeValue::Number(42.into()),
    ])]
fn test_walk_files_errors(#[case] build_args: fn(&str) -> Vec<RuntimeValue>) {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let args = build_args(&dir.path().to_string_lossy());
    assert!(call("walk_files", args).is_err());
}

#[cfg(feature = "file-io")]
#[test]
fn test_write_file_capability_gate_and_success() {
    let tmp = tempfile::NamedTempFile::new().expect("failed to create temp file");
    let path = tmp.path().to_string_lossy().to_string();
    assert!(
        call(
            "write_file",
            vec![
                RuntimeValue::String(Shared::new(path.clone())),
                RuntimeValue::String(Shared::new("hello".into()))
            ]
        )
        .is_err(),
        "write_file should be blocked when write access is not allowed"
    );

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_write(true)));
    assert_eq!(
        call(
            "write_file",
            vec![
                RuntimeValue::String(Shared::new(path.clone())),
                RuntimeValue::String(Shared::new("hello".into()))
            ]
        ),
        Ok(RuntimeValue::NONE)
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");

    assert_eq!(
        call(
            "write_file",
            vec![
                RuntimeValue::String(Shared::new(path.clone())),
                RuntimeValue::Bytes(Shared::new(vec![1, 2, 3]))
            ]
        ),
        Ok(RuntimeValue::NONE)
    );
    assert_eq!(std::fs::read(&path).unwrap(), vec![1, 2, 3]);

    let result = call(
        "write_file",
        vec![
            RuntimeValue::String(Shared::new("/nonexistent/dir/no_such_file.md".into())),
            RuntimeValue::String(Shared::new("hello".into())),
        ],
    );
    assert!(
        result.is_err(),
        "write_file should error when the parent directory doesn't exist"
    );
}

#[cfg(feature = "file-io")]
#[test]
fn test_mkdir_capability_gate_and_nested_idempotent_create() {
    let tmp = tempfile::tempdir().expect("failed to create temp dir");
    let dir = tmp.path().join("a").join("b");
    let args = || vec![RuntimeValue::String(Shared::new(dir.to_string_lossy().to_string()))];
    assert!(
        call("mkdir", args()).is_err(),
        "mkdir should be blocked when write access is not allowed"
    );
    assert!(!dir.exists());

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_write(true)));
    assert_eq!(call("mkdir", args()), Ok(RuntimeValue::NONE));
    assert!(dir.is_dir());
    assert_eq!(call("mkdir", args()), Ok(RuntimeValue::NONE));

    assert!(call("mkdir", vec![RuntimeValue::Number(1.into())]).is_err());
}

#[cfg(feature = "process-io")]
#[test]
fn test_system_capability_gate_and_success() {
    assert!(
        call("system", vec![RuntimeValue::String(Shared::new("echo".into()))]).is_err(),
        "system should be blocked when run access is not allowed"
    );

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_run(true)));

    let result = call(
        "system",
        vec![
            RuntimeValue::String(Shared::new("echo".into())),
            RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new("hello".into()))])),
        ],
    )
    .unwrap();
    assert_eq!(result, RuntimeValue::String(Shared::new("hello\n".to_string())));

    assert!(
        call(
            "system",
            vec![RuntimeValue::String(Shared::new(
                "mq-this-command-should-not-exist".into()
            ))]
        )
        .is_err(),
        "system should error for a missing command"
    );

    assert!(
        call(
            "system",
            vec![
                RuntimeValue::String(Shared::new("echo".into())),
                RuntimeValue::Array(Shared::new(vec![RuntimeValue::Number(1.into())])),
            ],
        )
        .is_err(),
        "system should error when args contains a non-string value"
    );

    assert!(call("system", vec![RuntimeValue::Number(42.into())]).is_err());
}

#[cfg(feature = "file-io")]
fn image_value(url: &str) -> RuntimeValue {
    RuntimeValue::Markdown(
        Shared::new(mq_markdown::Node::Image(mq_markdown::Image {
            alt: "alt".to_string(),
            url: url.to_string(),
            title: None,
            position: None,
        })),
        None,
    )
}

#[cfg(feature = "file-io")]
fn image_url(value: &RuntimeValue) -> String {
    match value {
        RuntimeValue::Markdown(node, _) => match &**node {
            mq_markdown::Node::Image(image) => image.url.clone(),
            other => panic!("expected Image node, got {other:?}"),
        },
        other => panic!("expected Markdown value, got {other:?}"),
    }
}

#[cfg(feature = "file-io")]
#[test]
fn test_embed_images_capability_gate_and_success() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    std::fs::write(dir.path().join("img.png"), [0x89, 0x50, 0x4e, 0x47]).expect("failed to write");
    let base_dir = dir.path().to_string_lossy().to_string();

    assert!(
        call(
            "embed_images",
            vec![
                image_value("img.png"),
                RuntimeValue::String(Shared::new(base_dir.clone()))
            ]
        )
        .is_err(),
        "embed_images should be blocked when read access is not allowed"
    );

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_read(true)));

    let result = call(
        "embed_images",
        vec![
            image_value("img.png"),
            RuntimeValue::String(Shared::new(base_dir.clone())),
        ],
    )
    .expect("embed_images should succeed");
    assert_eq!(image_url(&result), "data:image/png;base64,iVBORw==");

    // Default base_dir (".") combined with an absolute path.
    let absolute_path = dir.path().join("img.png").to_string_lossy().to_string();
    let result = call("embed_images", vec![image_value(&absolute_path)]).expect("embed_images should succeed");
    assert_eq!(image_url(&result), "data:image/png;base64,iVBORw==");

    // Already-embedded, remote, and non-image nodes pass through unchanged.
    let already_embedded = "data:image/png;base64,aGVsbG8=";
    assert_eq!(
        call(
            "embed_images",
            vec![
                image_value(already_embedded),
                RuntimeValue::String(Shared::new(base_dir.clone()))
            ]
        ),
        Ok(image_value(already_embedded))
    );
    assert_eq!(
        call(
            "embed_images",
            vec![
                image_value("https://example.com/img.png"),
                RuntimeValue::String(Shared::new(base_dir.clone()))
            ]
        ),
        Ok(image_value("https://example.com/img.png"))
    );
    let text_node = RuntimeValue::Markdown(
        Shared::new(mq_markdown::Node::Text(mq_markdown::Text {
            value: "hello".to_string(),
            position: None,
        })),
        None,
    );
    assert_eq!(
        call(
            "embed_images",
            vec![text_node.clone(), RuntimeValue::String(Shared::new(base_dir.clone()))]
        ),
        Ok(text_node)
    );

    let result = call(
        "embed_images",
        vec![
            image_value("no_such_file.png"),
            RuntimeValue::String(Shared::new(base_dir.clone())),
        ],
    );
    assert!(result.is_err(), "embed_images should error for a nonexistent file");

    let result = call(
        "embed_images",
        vec![image_value("img.unsupported"), RuntimeValue::String(base_dir.into())],
    );
    assert!(
        result.is_err(),
        "embed_images should error for an unsupported extension"
    );
}

#[cfg(feature = "file-io")]
#[test]
fn test_extract_images_capability_gate_and_success() {
    let bytes = [0x89, 0x50, 0x4e, 0x47];
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let data_url = format!("data:image/png;base64,{encoded}");
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let out_dir = dir.path().to_string_lossy().to_string();

    assert!(
        call(
            "extract_images",
            vec![
                image_value(&data_url),
                RuntimeValue::String(Shared::new(out_dir.clone()))
            ]
        )
        .is_err(),
        "extract_images should be blocked when write access is not allowed"
    );

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_write(true)));

    let result = call(
        "extract_images",
        vec![
            image_value(&data_url),
            RuntimeValue::String(Shared::new(out_dir.clone())),
        ],
    )
    .expect("extract_images should succeed");
    let hash = match convert::md5_bytes(&bytes).unwrap() {
        RuntimeValue::String(s) => s,
        _ => unreachable!(),
    };
    let expected_path = dir.path().join(format!("{hash}.png"));
    assert_eq!(image_url(&result), expected_path.to_string_lossy());
    assert_eq!(std::fs::read(&expected_path).unwrap(), bytes);

    // Non-data-URI and non-image nodes pass through unchanged.
    assert_eq!(
        call(
            "extract_images",
            vec![
                image_value("img.png"),
                RuntimeValue::String(Shared::new(out_dir.clone()))
            ]
        ),
        Ok(image_value("img.png"))
    );

    let result = call(
        "extract_images",
        vec![
            image_value("data:image/png;base64"),
            RuntimeValue::String(Shared::new(out_dir.clone())),
        ],
    );
    assert!(result.is_err(), "extract_images should error on a malformed data URI");

    let result = call(
        "extract_images",
        vec![
            image_value(&format!("data:image/png,{encoded}")),
            RuntimeValue::String(Shared::new(out_dir.clone())),
        ],
    );
    assert!(result.is_err(), "extract_images should error on a non-base64 data URI");

    let result = call(
        "extract_images",
        vec![
            image_value(&format!("data:application/pdf;base64,{encoded}")),
            RuntimeValue::String(out_dir.into()),
        ],
    );
    assert!(
        result.is_err(),
        "extract_images should error on an unsupported MIME type"
    );
}

#[cfg(all(feature = "http", feature = "mock-io"))]
#[test]
fn test_mock_fetch_seeds_a_response_the_http_builtin_then_reads_back() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));

    assert_eq!(
        call(
            "mock_fetch",
            vec![
                RuntimeValue::String(Shared::new("https://example.invalid".into())),
                RuntimeValue::String(Shared::new("body".into())),
            ]
        ),
        Ok(RuntimeValue::NONE)
    );
    assert_eq!(
        call(
            "http",
            vec![
                RuntimeValue::Symbol(Ident::new("get")),
                RuntimeValue::String(Shared::new("https://example.invalid".into())),
            ]
        ),
        Ok(RuntimeValue::String(Shared::new("body".into())))
    );
}

#[test]
fn test_env_returns_a_dict_restricted_to_allowed_names() {
    let _guard = io_context::scoped(Shared::new(
        SandboxedIo::new(MemIo::default().with_env("MQ_TEST_VAR", "value").with_env("OTHER", "x"))
            .allow_env(vec!["MQ_TEST_VAR".to_string()]),
    ));

    assert_eq!(
        call("env", vec![]),
        Ok(RuntimeValue::from(DictMap::from_iter([(
            Ident::new("MQ_TEST_VAR"),
            RuntimeValue::String(Shared::new("value".into()))
        )])))
    );
}

#[test]
fn test_env_with_name_returns_the_value_or_none_if_unset() {
    let _guard = io_context::scoped(Shared::new(
        SandboxedIo::new(MemIo::default().with_env("MQ_TEST_VAR", "value")).allow_env(true),
    ));

    assert_eq!(
        call("env", vec![RuntimeValue::String(Shared::new("MQ_TEST_VAR".into()))]),
        Ok(RuntimeValue::String(Shared::new("value".into())))
    );
    assert_eq!(
        call("env", vec![RuntimeValue::String(Shared::new("MQ_MISSING".into()))]),
        Ok(RuntimeValue::NONE)
    );
}

#[test]
fn test_env_with_name_errors_when_not_in_the_allowlist() {
    let _guard = io_context::scoped(Shared::new(
        SandboxedIo::new(MemIo::default().with_env("MQ_TEST_VAR", "value")).allow_env(vec!["OTHER".to_string()]),
    ));
    assert!(call("env", vec![RuntimeValue::String(Shared::new("MQ_TEST_VAR".into()))]).is_err());
}

#[test]
fn test_env_errors_when_env_access_is_denied() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default())));
    assert!(call("env", vec![]).is_err());
}

#[cfg(all(feature = "http", feature = "mock-io"))]
#[test]
fn test_open_http_streams_the_mocked_response_line_by_line() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));
    call(
        "mock_fetch",
        vec![
            RuntimeValue::String(Shared::new("https://example.invalid".into())),
            RuntimeValue::String(Shared::new("a\nb\nc".into())),
        ],
    )
    .unwrap();

    let handle = call(
        "open_http",
        vec![
            RuntimeValue::Symbol(Ident::new("get")),
            RuntimeValue::String(Shared::new("https://example.invalid".into())),
        ],
    )
    .unwrap();
    assert_eq!(call("type", vec![handle.clone()]).unwrap().to_string(), "http");

    for line in ["a", "b", "c"] {
        assert_eq!(
            call("read_line", vec![handle.clone()]),
            Ok(RuntimeValue::String(Shared::new(line.into())))
        );
    }
    assert_eq!(call("read_line", vec![handle.clone()]), Ok(RuntimeValue::NONE));

    assert!(call("close", vec![handle.clone()]).is_ok());
    assert_eq!(
        call("status", vec![handle]),
        Ok(RuntimeValue::Symbol(Ident::new("closed")))
    );
}

#[cfg(all(feature = "http", feature = "mock-io"))]
#[test]
fn test_http_all_fetches_each_request_in_batch_order() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));
    for (url, body) in [
        ("https://example.invalid/a", "body-a"),
        ("https://example.invalid/b", "body-b"),
        ("https://example.invalid/c", "body-c"),
    ] {
        call(
            "mock_fetch",
            vec![
                RuntimeValue::String(Shared::new(url.into())),
                RuntimeValue::String(Shared::new(body.into())),
            ],
        )
        .unwrap();
    }

    let requests = RuntimeValue::Array(Shared::new(vec![
        RuntimeValue::Dict(Shared::new(DictMap::from_iter([(
            Ident::new("url"),
            RuntimeValue::String(Shared::new("https://example.invalid/a".into())),
        )]))),
        RuntimeValue::Dict(Shared::new(DictMap::from_iter([(
            Ident::new("url"),
            RuntimeValue::String(Shared::new("https://example.invalid/b".into())),
        )]))),
        RuntimeValue::Dict(Shared::new(DictMap::from_iter([
            (Ident::new("method"), RuntimeValue::Symbol(Ident::new("post"))),
            (
                Ident::new("url"),
                RuntimeValue::String(Shared::new("https://example.invalid/c".into())),
            ),
            (Ident::new("body"), RuntimeValue::String(Shared::new("payload".into()))),
        ]))),
    ]));

    assert_eq!(
        call("http_all", vec![requests]),
        Ok(RuntimeValue::Array(Shared::new(vec![
            RuntimeValue::String(Shared::new("body-a".into())),
            RuntimeValue::String(Shared::new("body-b".into())),
            RuntimeValue::String(Shared::new("body-c".into())),
        ])))
    );
}

#[cfg(all(feature = "http", feature = "mock-io"))]
#[test]
fn test_http_all_rejects_non_dict_request() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));

    let requests = RuntimeValue::Array(Shared::new(vec![RuntimeValue::String(Shared::new(
        "https://example.invalid".into(),
    ))]));

    assert!(call("http_all", vec![requests]).is_err());
}

#[cfg(all(feature = "http", feature = "mock-io"))]
#[test]
fn test_http_all_rejects_request_without_url() {
    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(MemIo::default()).allow_net(true)));

    let requests = RuntimeValue::Array(Shared::new(vec![RuntimeValue::Dict(Shared::new(DictMap::from_iter(
        [(Ident::new("method"), RuntimeValue::Symbol(Ident::new("get")))],
    )))]));

    assert!(call("http_all", vec![requests]).is_err());
}

#[cfg(all(feature = "http", feature = "mock-io"))]
#[test]
fn test_mock_fetch_is_refused_by_non_mock_io() {
    assert!(
        call(
            "mock_fetch",
            vec![
                RuntimeValue::String(Shared::new("https://example.invalid".into())),
                RuntimeValue::String(Shared::new("body".into())),
            ]
        )
        .is_err(),
        "mock_fetch should be refused when the ambient Io isn't a mock"
    );

    let _guard = io_context::scoped(Shared::new(SandboxedIo::new(NativeIo::default()).allow_net(true)));
    assert!(
        call(
            "mock_fetch",
            vec![
                RuntimeValue::String(Shared::new("https://example.invalid".into())),
                RuntimeValue::String(Shared::new("body".into())),
            ]
        )
        .is_err(),
        "mock_fetch should be refused against a real, network-backed Io"
    );
}

#[rstest]
#[case::ascii("hello", "l", 2, 3)]
#[case::latin("héllo", "l", 2, 3)]
#[case::cjk("日本語", "語", 2, 2)]
#[case::missing("héllo", "z", -1, -1)]
fn test_string_index_rindex_use_char_offsets(
    #[case] haystack: &str,
    #[case] needle: &str,
    #[case] index: i64,
    #[case] rindex: i64,
) {
    let args = || vec![RuntimeValue::from(haystack), RuntimeValue::from(needle)];
    assert_eq!(call("index", args()), Ok(RuntimeValue::Number(index.into())));
    assert_eq!(call("rindex", args()), Ok(RuntimeValue::Number(rindex.into())));
}

#[rstest]
#[case::ascii("abab", "ab", vec![0, 2])]
#[case::latin("héllo", "l", vec![2, 3])]
#[case::cjk("日本語語", "語", vec![2, 3])]
#[case::mixed("é語é語", "語", vec![1, 3])]
fn test_string_indices_use_char_offsets(#[case] haystack: &str, #[case] needle: &str, #[case] expected: Vec<i64>) {
    let expected = RuntimeValue::Array(Shared::new(
        expected.into_iter().map(|i| RuntimeValue::Number(i.into())).collect(),
    ));
    assert_eq!(
        call(
            "indices",
            vec![RuntimeValue::from(haystack), RuntimeValue::from(needle)]
        ),
        Ok(expected)
    );
}

#[rstest]
#[case::string(RuntimeValue::from("héllo"), RuntimeValue::from("ll"), true)]
#[case::string_empty_needle(RuntimeValue::from("abc"), RuntimeValue::from(""), true)]
#[case::string_missing(RuntimeValue::from("abc"), RuntimeValue::from("z"), false)]
#[case::bytes(RuntimeValue::Bytes(vec![1, 2, 3].into()), RuntimeValue::Bytes(vec![2, 3].into()), true)]
#[case::bytes_empty_needle(RuntimeValue::Bytes(vec![1, 2].into()), RuntimeValue::Bytes(Vec::new().into()), false)]
#[case::array(RuntimeValue::Array(Shared::new(vec![1.into(), 2.into()])), RuntimeValue::Number(2.into()), true)]
#[case::array_missing(RuntimeValue::Array(Shared::new(vec![1.into()])), RuntimeValue::Number(2.into()), false)]
#[case::none(RuntimeValue::NONE, RuntimeValue::from("a"), false)]
fn test_contains(#[case] haystack: RuntimeValue, #[case] needle: RuntimeValue, #[case] expected: bool) {
    assert_eq!(call("contains", vec![haystack, needle]), Ok(expected.into()));
}

#[rstest]
#[case::present(RuntimeValue::from("a"), true)]
#[case::none_value(RuntimeValue::from("n"), false)]
#[case::missing(RuntimeValue::from("z"), false)]
#[case::non_key_type(RuntimeValue::Number(1.into()), false)]
fn test_contains_dict(#[case] key: RuntimeValue, #[case] expected: bool) {
    let mut map = DictMap::default();
    map.insert(Ident::new("a"), RuntimeValue::Number(1.into()));
    map.insert(Ident::new("n"), RuntimeValue::NONE);
    assert_eq!(
        call("contains", vec![RuntimeValue::Dict(Shared::new(map)), key]),
        Ok(expected.into())
    );
}

proptest::proptest! {
    /// Long enough to cross `MIN_INDEXED_LEN`, with multi-byte chars to exercise the sparse index.
    #[test]
    fn test_char_at_and_slice_str_match_a_char_vector(
        text in "[aあ😀b]{0,1500}",
        start in -1600i64..1600,
        end in -1600i64..1600,
    ) {
        let s = Shared::new(text);
        let chars: Vec<char> = s.chars().collect();
        let (start, end) = (number::Number::from(start), number::Number::from(end));

        let expected: String = chars[slice_bounds(start, end, chars.len())].iter().collect();
        proptest::prop_assert_eq!(slice_str(&s, start, end), expected);
        proptest::prop_assert_eq!(
            char_at(&s, start),
            resolve_index(start, chars.len()).and_then(|i| chars.get(i).copied())
        );
    }

    #[test]
    fn test_slice_bounds_is_a_valid_range(
        start in proptest::num::f64::ANY,
        end in proptest::num::f64::ANY,
        len in 0usize..=isize::MAX as usize,
    ) {
        let range = slice_bounds(number::Number::new(start), number::Number::new(end), len);
        proptest::prop_assert!(range.start <= range.end && range.end <= len);
    }

    /// Only a negative index is bounded by `len`; callers range-check a non-negative one with `get`.
    #[test]
    fn test_resolve_index_rejects_non_integers_and_bounds_negative_indexes(
        value in proptest::num::f64::ANY,
        len in 0usize..=isize::MAX as usize,
    ) {
        let index = number::Number::new(value);
        let resolved = resolve_index(index, len);
        if !index.is_int() {
            proptest::prop_assert!(resolved.is_none());
        } else if (value as isize) < 0 && let Some(i) = resolved {
            proptest::prop_assert!(i < len);
        }
    }
}

/// A non-ASCII char at every position of strings whose lengths straddle the word and vector widths
/// `is_ascii` scans in, so a char in a chunk, in the tail or right at a boundary is still counted.
#[test]
fn test_count_chars_is_right_wherever_a_non_ascii_char_sits() {
    for len in [0usize, 1, 2, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127, 128, 129] {
        let ascii = "a".repeat(len);
        assert_eq!(char_index::count_chars(&ascii), len);
        for at in 0..len {
            for wide in ["é", "あ", "😀"] {
                let mut text = String::new();
                text.push_str(&ascii[..at]);
                text.push_str(wide);
                text.push_str(&ascii[at + 1..]);
                assert_eq!(char_index::count_chars(&text), len, "{wide} at {at} of {len}");
            }
        }
    }
}

proptest::proptest! {
    #[test]
    fn test_count_chars_matches_chars_count(text in "[aあ😀é]{0,400}") {
        proptest::prop_assert_eq!(char_index::count_chars(&text), text.chars().count());
    }

    #[test]
    fn test_count_chars_matches_chars_count_for_mostly_ascii_text(text in "[a-z ]{0,300}[あ😀é]?[a-z ]{0,300}") {
        proptest::prop_assert_eq!(char_index::count_chars(&text), text.chars().count());
    }
}

#[rstest]
#[case::ascii("hello", 5)]
#[case::empty("", 0)]
#[case::japanese("日本語", 3)]
#[case::emoji("a😀b", 3)]
#[case::combining("e\u{301}", 2)]
#[case::non_ascii_last(format!("{}あ", "a".repeat(2000)), 2001)]
#[case::non_ascii_first(format!("あ{}", "a".repeat(2000)), 2001)]
fn test_len_counts_chars_not_bytes(#[case] text: impl Into<String>, #[case] expected: usize) {
    let s = RuntimeValue::String(Shared::new(text.into()));
    assert_eq!(
        eval_builtin(
            &RuntimeValue::None,
            &Ident::new("len"),
            vec![s].into(),
            &VmEnv::default()
        ),
        Ok(RuntimeValue::Number(expected.into()))
    );
}

#[test]
fn test_char_at_sees_mutation_of_an_indexed_string() {
    let mut s = Shared::new("a".repeat(2000));
    assert_eq!(char_at(&s, number::Number::from(-1)), Some('a'));

    Shared::make_mut(&mut s).push('あ');
    assert_eq!(char_at(&s, number::Number::from(-1)), Some('あ'));
    assert_eq!(char_at(&s, number::Number::from(2000)), Some('あ'));
}

// =========================================================================
// band, bor, xor on numbers
// =========================================================================

#[rstest]
#[case::band("band", 12, 10, 8)]
#[case::bor("bor", 12, 10, 14)]
#[case::xor("xor", 12, 10, 6)]
#[case::band_with_zero("band", 9_007_199_254_740_991, 0, 0)]
#[case::band_keeps_the_low_bits("band", 9_007_199_254_740_991, 12, 12)]
#[case::bor_above_32_bits("bor", 4_503_599_627_370_496, 1, 4_503_599_627_370_497)]
#[case::xor_clears_the_same_bits("xor", 9_007_199_254_740_991, 9_007_199_254_740_991, 0)]
#[case::xor_flips_the_low_bit("xor", 9_007_199_254_740_991, 1, 9_007_199_254_740_990)]
fn test_bitwise_numbers(#[case] name: &str, #[case] a: i64, #[case] b: i64, #[case] expected: i64) {
    assert_eq!(
        call(
            name,
            vec![RuntimeValue::Number(a.into()), RuntimeValue::Number(b.into())]
        ),
        Ok(RuntimeValue::Number(expected.into()))
    );
}

#[rstest]
#[case::negative("band", (-1).into(), "-1")]
#[case::too_large("bor", 9_007_199_254_740_992i64.into(), "9007199254740992")]
#[case::fraction("xor", 1.5.into(), "1.5")]
fn test_bitwise_numbers_reject_what_they_cannot_hold(
    #[case] name: &str,
    #[case] bad: crate::number::Number,
    #[case] shown: &str,
) {
    assert_eq!(
        call(name, vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(bad)]),
        Err(Error::Runtime(format!(
            "{name}: expected an integer from 0 to 2^53 - 1, got {shown}"
        )))
    );
}

#[test]
fn test_bitwise_numbers_do_not_mix_with_bytes() {
    assert!(
        call(
            "band",
            vec![
                RuntimeValue::Number(1.into()),
                RuntimeValue::Bytes(Shared::new(vec![1]))
            ]
        )
        .is_err()
    );
}
