//! Shared input generation for the fuzz targets: `interpreter` feeds generated scripts
//! through [`mq_lang::DefaultEngine`], and `mqc` loads corrupted `.mqc` files.

use arbitrary::Arbitrary;
use itertools::Itertools;

#[derive(Debug, Clone, Arbitrary)]
pub enum Expr {
    Let(String, String),
    Def(String, Vec<String>, String),
    Call(String, Vec<String>),
    Raw(String),
}

#[derive(Debug, Clone, Arbitrary)]
pub struct ArbitraryScript {
    exprs: Vec<Expr>,
}

impl ArbitraryScript {
    fn to_script(&self) -> String {
        let mut script = String::new();
        for stmt in &self.exprs {
            match stmt {
                Expr::Let(name, value) => {
                    script.push_str(&format!("let {} = {}\n", name, value));
                }
                Expr::Call(name, args) => {
                    let args_str = args.join(", ");
                    script.push_str(&format!("{}({})", name, args_str));
                }
                Expr::Def(name, args, body) => {
                    let args_str = args.join(", ");
                    script.push_str(&format!("def {}({}) {{ {} }};\n", name, args_str, body));
                }
                Expr::Raw(code) => {
                    script.push_str(code);
                    script.push('\n');
                }
            }
        }
        script
    }
}

#[derive(Debug, Clone, Arbitrary)]
pub struct Context {
    raw_script: Option<String>,
    generated_script: Option<Vec<ArbitraryScript>>,
}

impl Context {
    pub fn to_script(&self) -> String {
        match (&self.raw_script, &self.generated_script) {
            (Some(raw), _) => raw.clone(),
            (_, Some(generated)) => generated.iter().map(|g| g.to_script()).join(" | "),
            _ => "".to_string(),
        }
    }
}

/// Evaluates `context`'s script under a `catch_unwind`, panicking (with the context printed)
/// if the engine panics instead of returning a `Result`.
pub fn eval_and_check(context: &Context) {
    let script = context.to_script();

    let result = std::panic::catch_unwind(|| {
        let mut engine = mq_lang::DefaultEngine::default();
        let _ = engine.eval(&script, mq_lang::null_input().into_iter());
    });

    if let Err(err) = result {
        println!("Fuzzing with context: {:?}", context);
        std::panic::resume_unwind(err);
    }
}

/// Programs that together compile to every VM instruction (the same set as mq-lang's
/// `mqc::fuzz_tests`), compiled to `.mqc` and then corrupted.
const MQC_PROGRAMS: &[&str] = &[
    r#"def f(x): x + 1; | [f(1), .h1, s"${self}"]"#,
    "let add = fn(a): fn(b): a + b;; | let inc = add(1) | inc(2)",
    "def fact(n): if (n <= 1): 1 else: n * fact(n - 1); | fact(5)",
    "var s = 0 | foreach(x, range(1, 10)): s += x; | s",
    r#"try: error("boom") catch(e): s"caught ${e}""#,
    "def gen(): yield 1 | yield 2; | let g = gen() | [next(g), next(g)]",
    r#"def greet(name, greeting = "hi"): s"${greeting} ${name}"; | greet("mq")"#,
    r#"{"a": 1, "b": [1, 2, {"c": true}]}"#,
    ".h | let last = to_text() | nodes | last",
    "def f(a, b): [a - b, a / b, a % b, a == b, a != b, a < b, a <= b, a > b, a >= b, -a, !a]; | f(7, 2)",
    r#"[len("abc") - 1, len("abcd") / 2, len("abc") % 2, len("a") != 1, len("a") < 2, len("a") <= 2, len("ab") >= 2]"#,
    r#"[unless (len("a") == 0): "x", do var n = 0 | until (n >= 2): n += 1; | n end]"#,
    r#"let t = true | [t && len("a") > 0, t || len("") > 0]"#,
    r#"def add(a, b): a + b; | def suffix(a): a + "!"; | [add(1, 2), suffix("x")]"#,
    r#"def f(a): [a + "x"]; | f("y")"#,
    r#"def eqs(s): if (s == "a"): 1 else: 2; | [eqs("a"), eqs("b")]"#,
    r#"var i = 0 | var s = "" | while (i < 5): i += 2 | s += "x"; | [i, s]"#,
    "def lt(a, b): var n = 0 | while (a < b): a += 1 | n += 1; | n; | lt(1, 4)",
    "var c = 0 | let inc = fn(): c += 1; | inc() | inc() | c",
    r#"let a = [1, 2] | let b = [3] | let d = {"x": 1} | [[...a, ...b, 0], {...d, "y": 2}]"#,
    r#"let [x, y] = [1, 2] | let {name} = {"name": "n"} | [x, y, name]"#,
    "let [p, q] = [1] | p",
    "let arr = [1, 2, 3] | [len(arr), arr[1], arr[0:2]]",
    "def at(xs, i): get(xs, i); | at([1, 2], 0)",
    "let xs = [1, 2, 3] | foreach(x, xs): x * 2;",
    "var total = 0 | foreach(x, [1, 2, 3]): total += x; | total",
    "foreach(x, [1, 2, 3]): if (x > 1): x else: 0;",
    r#"let v = 1 | match (v): | :string: "s" | :number: "n" | _: "o" end"#,
    "match ([1, 2]): | [a, b]: a + b | _: 0 end",
    "match ([1, 2, 3]): | [first, ..rest]: rest | _: [] end",
    r#"let e = s"${$HOME}" | e"#,
    "some_unknown_global",
    r#"[.h, .code("rust"), .[0], .link.url, .h(2), .list]"#,
    "def z(): 1; | def two(a, b): a; | def three(a, b, c): c; | [z(), two(1, 2), three(1, 2, 3)]",
    r#"def two(a, b): a; | if (len("") > 0): two(1, 2, 3) else: 0"#,
    "def opt(a, b = 1): a + b; | opt(1, 2)",
    r#"def up(s): upcase(s); | "a" | up()"#,
    r#"def g(x): upcase(x); | g("a")"#,
    "def countdown(n): if (n <= 0): 0 else: countdown(n - 1); | def pair(a, b): if (a <= 0): b else: pair(a - 1, b + 1); | def zero(): 0; | [countdown(3), pair(2, 0), zero()]",
    "def r(n, acc = 0): if (n <= 0): acc else: r(n - 1, acc + n); | r(3)",
    "def r(n): if (n <= 0): 0 else: r(n - 1, 1); | r(0)",
    "def r3(a, b, c): if (a <= 0): c else: r3(a - 1, b, c + 1); | r3(2, 0, 0)",
    "def dec(n): if (n <= 0): 0 else: do n - 1 | dec() end; | dec(2)",
    r#"def self_call(): len(); | "abc" | self_call()"#,
    r#"def rec(): if (len() > 0): 1 else: rec(); | "a" | rec()"#,
    "let h = fn(x): x + 1; | let apply = fn(v): h(v); | apply(1)",
    "def outer(): let k = fn(): 1; | fn(): k();; | let m = outer() | m()",
    "var g = fn(x): x; | g(1)",
    "module m: let a = 1 | def f(): a; end | m::f()",
    "var n = 0 | while (n < 5): n += 1 | if (n == 2): continue | if (n == 4): break;",
    "var n = 0 | while (n < 5): n += 1 | try: continue catch: 0; | n",
    "var n = 0 | while (true): n += 1 | try: break catch: 0; | n",
    "foreach(x, [1, 2, 3]): try: do if (x == 2): continue else: x end catch: 0;",
    "foreach(x, [1, 2, 3]): try: do if (x == 2): break else: x end catch: 0;",
];

/// A `.mqc` file compiled from one of the fuzz programs, then corrupted.
#[derive(Debug, Clone, Arbitrary)]
pub struct MqcInput {
    program: u8,
    /// Byte overwrites, placed by position modulo the file's content length.
    edits: Vec<(u32, u8)>,
}

/// Loads the corrupted `.mqc` file and, when it still loads, runs it.
///
/// The checksum is recomputed after the edits, so the input reaches the decoder and the
/// bytecode verifier. Anything that passes verification must run without panicking.
pub fn load_and_run_mqc(input: &MqcInput) {
    use sha2::{Digest, Sha256};
    const CHECKSUM_LEN: usize = 32;

    let program = MQC_PROGRAMS[usize::from(input.program) % MQC_PROGRAMS.len()];
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    let mut bytes = engine
        .precompile(program, &[])
        .expect("fuzz programs compile")
        .into_bytes();
    let content_len = bytes.len() - CHECKSUM_LEN;
    for (position, value) in &input.edits {
        bytes[*position as usize % content_len] = *value;
    }
    let checksum = Sha256::digest(&bytes[..content_len]);
    bytes[content_len..].copy_from_slice(&checksum);

    let Ok(mqc) = mq_lang::Mqc::try_from(bytes) else {
        return;
    };
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(std::time::Duration::from_millis(100));
    engine.set_max_call_stack_depth(64);
    if let Ok(program) = engine.load(&mqc) {
        let input = mq_lang::parse_markdown_input("# a\n\n## b\n").unwrap();
        let _ = engine.eval_compiled(&program, input.into_iter());
    }
}
