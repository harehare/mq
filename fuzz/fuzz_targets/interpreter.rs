#![no_main]

use arbitrary::Arbitrary;
use itertools::Itertools;
use libfuzzer_sys::fuzz_target;

#[derive(Debug, Clone, Arbitrary)]
enum Expr {
    Let(String, String),
    Def(String, Vec<String>, String),
    Call(String, Vec<String>),
    Raw(String),
}

#[derive(Debug, Clone, Arbitrary)]
struct ArbitraryScript {
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
struct Context {
    raw_script: Option<String>,
    generated_script: Option<Vec<ArbitraryScript>>,
}

fuzz_target!(|context: Context| {
    let script = match (&context.raw_script, &context.generated_script) {
        (Some(raw), _) => raw.clone(),
        (_, Some(generated)) => generated.iter().map(|g| g.to_script()).join(" | "),
        _ => "".to_string(),
    };

    let mut engine = mq_lang::Engine::default();
    let _ = engine.eval(&script, vec![mq_lang::RuntimeValue::String("".to_string())].into_iter());
});
