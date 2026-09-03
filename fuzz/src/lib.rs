//! Shared arbitrary-script generation for the `interpreter` and `tarn` fuzz targets.
//! Both targets feed the same generated scripts through [`mq_lang::DefaultEngine`]; which
//! evaluator they exercise (tree-walker vs. the `tarn` bytecode VM) is decided by whether
//! `mq-lang/tarn` is enabled for the build.

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

    if result.is_err() {
        println!("Fuzzing with context: {:?}", context);
        result.unwrap();
    }
}
