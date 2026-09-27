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

/// Programs covering the VM's instruction families, compiled to `.mqc` and then corrupted.
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
