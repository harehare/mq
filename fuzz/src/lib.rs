//! Shared arbitrary-script generation for the `interpreter` and `tarn` fuzz targets.
//! Both targets feed the same generated scripts through [`mq_lang::DefaultEngine`]; which
//! evaluator they exercise (tree-walker vs. the `tarn` bytecode VM) is decided by whether
//! `mq-lang/tarn` is enabled for the build.

use arbitrary::{Arbitrary, Unstructured};
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

/// A deterministic mq program shared by the tree-walker and Tarn differential fuzz target.
///
/// The generator intentionally uses only language constructs supported by both execution
/// engines. This makes a reported mismatch actionable rather than an expected consequence of
/// fuzzing an implementation-specific feature.
#[derive(Debug, Clone)]
pub struct DifferentialContext {
    values: [i16; 4],
    selector: u8,
}

impl<'a> Arbitrary<'a> for DifferentialContext {
    fn arbitrary(unstructured: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        // libFuzzer begins from an empty corpus and therefore generates very short inputs
        // first. Zero-fill missing bytes so every input reaches an engine comparison instead
        // of being discarded by a fixed-size `Arbitrary` derivation.
        let mut bytes = [0_u8; 9];
        for byte in &mut bytes {
            *byte = unstructured.arbitrary::<u8>().unwrap_or(0);
        }

        Ok(Self {
            values: [
                i16::from_le_bytes([bytes[0], bytes[1]]),
                i16::from_le_bytes([bytes[2], bytes[3]]),
                i16::from_le_bytes([bytes[4], bytes[5]]),
                i16::from_le_bytes([bytes[6], bytes[7]]),
            ],
            selector: bytes[8],
        })
    }
}

impl DifferentialContext {
    /// Renders this context as a valid, deterministic mq script.
    pub fn to_script(&self) -> String {
        let [a, b, c, extra] = self.values;
        let prelude = format!("let a = {a} | let b = {b} | let c = {c} | let extra = {extra} | ");

        let body = match self.selector % 16 {
            0 => "a + b * c".to_owned(),
            1 => "if (a < b): a - c else: b + c".to_owned(),
            2 => format!("let values = [a, b, c] | get(values, {})", extra.rem_euclid(3)),
            3 => "let f = fn(value): a + value * c; | f(b)".to_owned(),
            4 => format!("var total = a | foreach(value, [b, c, {extra}]): total += value; | total"),
            5 => "let [first, second] = [a, b] | first * second + c".to_owned(),
            6 => format!("len([a, b, c, {extra}])"),
            7 => "let distance = fn(value): if (value > a): value - a else: a - value; | distance(b)".to_owned(),
            8 => "if ((a < b && b < c) || a == extra): a + b else: c - extra".to_owned(),
            9 => "if (a < b): if (b < c): a + b + c else: a + b - c else: if (a == c): extra else: a - c".to_owned(),
            10 => "let values = [a, b, c, extra] | values | filter(fn(value): (value > a && value < b) || value == c;) | map(fn(value): value + c;) | filter(fn(value): value != extra || value == a;)".to_owned(),
            11 => "var total = 0 | foreach(value, [a, b, c, extra]): if ((value > a && value < b) || value == c): total += value else: total -= value; | total".to_owned(),
            12 => "let qualifies = fn(value): (value >= a && value <= b) || value == c; | if (qualifies(extra) && extra != a): extra + c else: extra - c".to_owned(),
            13 => "match([a, b]) do | [first, second] if (first < second && second != c): first + second | [first, _] if (first == c || first == extra): first | _: c end".to_owned(),
            14 => "(a < b && b < c) || (extra == a && c != b)".to_owned(),
            _ => "let values = [a, b, c, extra] | values | filter(fn(value): value >= a && value <= c;) | map(fn(value): if (value == b || value == extra): value * 2 else: value - 1;)".to_owned(),
        };

        format!("{prelude}{body}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn differential_scripts_evaluate_for_every_variant() {
        for selector in 0..16 {
            let context = DifferentialContext {
                values: [-3, 5, 2, 7],
                selector,
            };
            let script = context.to_script();
            let mut engine = mq_lang::DefaultEngine::default();
            engine.load_builtin_module();
            let result = engine.eval(&script, mq_lang::null_input().into_iter());

            assert!(
                result.is_ok(),
                "generated script failed to evaluate: {script}\nerror: {result:?}"
            );
        }
    }

    #[test]
    fn differential_context_accepts_short_fuzzer_inputs() {
        for input_length in 0..9 {
            let bytes = vec![0; input_length];
            let mut input = Unstructured::new(&bytes);

            assert!(DifferentialContext::arbitrary(&mut input).is_ok());
        }
    }
}
