//! Grammar-based generation of well-formed mq programs and Markdown documents.
//!
//! Unlike the token-soup inputs in the crate root, every value built here parses, so the
//! fuzzer spends its time in the compiler, the VM and the renderers instead of the parser's
//! error paths.

use arbitrary::{Arbitrary, Result, Unstructured};

const MAX_DEPTH: u32 = 5;
const MAX_WIDTH: usize = 4;
const VARS: &[&str] = &["a", "b", "c", "xs", "d"];
const BINARY_OPS: &[&str] = &[
    "+", "-", "*", "/", "%", "==", "!=", "<", "<=", ">", ">=", "&&", "||", "??",
];
/// Builtins with their arity. Only pure ones, so that a result is comparable across engines.
const CALLS: &[(&str, usize)] = &[
    ("len", 1),
    ("upcase", 1),
    ("downcase", 1),
    ("to_string", 1),
    ("to_number", 1),
    ("first", 1),
    ("last", 1),
    ("reverse", 1),
    ("sort", 1),
    ("abs", 1),
    ("keys", 1),
    ("values", 1),
    ("flatten", 1),
    ("compact", 1),
    ("get", 2),
    ("add", 2),
    ("join", 2),
    ("split", 2),
    ("contains", 2),
    ("range", 2),
    ("slice", 3),
    ("replace", 3),
];
const MATCH_TYPES: &[&str] = &["string", "number", "array", "dict", "bool", "none"];

/// A generated mq expression.
#[derive(Debug, Clone)]
pub enum Expr {
    Num(i16),
    Str(String),
    Bool(bool),
    Null,
    Var(usize),
    Array(Vec<Expr>),
    Dict(Vec<(String, Expr)>),
    Unary(bool, Box<Expr>),
    Binary(usize, Box<Expr>, Box<Expr>),
    Call(usize, Vec<Expr>),
    If(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `let v = value | body`
    Let(usize, Box<Expr>, Box<Expr>),
    /// `def f(a, b): body; | rest`, then `f` is called in `rest`.
    Def(Box<Expr>, Box<Expr>),
    Lambda(Box<Expr>, Box<Expr>),
    Foreach(Box<Expr>, Box<Expr>),
    Match(Box<Expr>, Vec<(usize, Expr)>, Box<Expr>),
    Try(Box<Expr>, Box<Expr>),
    Pipe(Box<Expr>, Box<Expr>),
}

fn ident(u: &mut Unstructured) -> Result<usize> {
    u.choose_index(VARS.len())
}

/// Strings stay printable so the escape sequences that the lexer handles are exercised
/// without making most programs invalid.
fn string(u: &mut Unstructured) -> Result<String> {
    const CHARS: &[char] = &['a', 'b', 'Z', '0', ' ', ',', '\\', '"', '\n', '$', '{', '日'];
    let len = u.int_in_range(0..=6)?;
    (0..len).map(|_| Ok(*u.choose(CHARS)?)).collect()
}

impl Expr {
    fn generate(u: &mut Unstructured, depth: u32) -> Result<Self> {
        let sub = |u: &mut Unstructured| Ok(Box::new(Expr::generate(u, depth + 1)?));
        let list = |u: &mut Unstructured, len: usize| {
            (0..len)
                .map(|_| Expr::generate(u, depth + 1))
                .collect::<Result<Vec<_>>>()
        };
        let leaf = depth >= MAX_DEPTH || u.ratio(1, 4)?;
        if leaf {
            return Ok(match u.int_in_range(0..=4)? {
                0 => Expr::Num(u.arbitrary()?),
                1 => Expr::Str(string(u)?),
                2 => Expr::Bool(u.arbitrary()?),
                3 => Expr::Null,
                _ => Expr::Var(ident(u)?),
            });
        }
        Ok(match u.int_in_range(0..=13)? {
            0 => {
                let len = u.int_in_range(0..=MAX_WIDTH)?;
                Expr::Array(list(u, len)?)
            }
            1 => {
                let len = u.int_in_range(0..=MAX_WIDTH)?;
                let mut entries = Vec::with_capacity(len);
                for _ in 0..len {
                    entries.push((string(u)?, Expr::generate(u, depth + 1)?));
                }
                Expr::Dict(entries)
            }
            2 => Expr::Unary(u.arbitrary()?, sub(u)?),
            3 | 4 => Expr::Binary(u.choose_index(BINARY_OPS.len())?, sub(u)?, sub(u)?),
            5 => {
                let call = u.choose_index(CALLS.len())?;
                Expr::Call(call, list(u, CALLS[call].1)?)
            }
            6 => Expr::If(sub(u)?, sub(u)?, sub(u)?),
            7 => Expr::Let(ident(u)?, sub(u)?, sub(u)?),
            8 => Expr::Def(sub(u)?, sub(u)?),
            9 => Expr::Lambda(sub(u)?, sub(u)?),
            10 => Expr::Foreach(sub(u)?, sub(u)?),
            11 => {
                let len = u.int_in_range(1..=3)?;
                let mut arms = Vec::with_capacity(len);
                for _ in 0..len {
                    arms.push((u.choose_index(MATCH_TYPES.len())?, Expr::generate(u, depth + 1)?));
                }
                Expr::Match(sub(u)?, arms, sub(u)?)
            }
            12 => Expr::Try(sub(u)?, sub(u)?),
            _ => Expr::Pipe(sub(u)?, sub(u)?),
        })
    }

    /// Writes the expression as mq source. Operands are parenthesized, and the program shapes
    /// that need `;` or `end` are closed, so precedence never decides what the program means.
    pub fn render(&self, out: &mut String) {
        match self {
            Expr::Num(n) => out.push_str(&n.to_string()),
            Expr::Str(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '\\' => out.push_str("\\\\"),
                        '"' => out.push_str("\\\""),
                        '\n' => out.push_str("\\n"),
                        '$' => out.push_str("\\$"),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Expr::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Expr::Null => out.push_str("null"),
            Expr::Var(i) => out.push_str(VARS[*i]),
            Expr::Array(items) => {
                out.push('[');
                Self::render_list(items, out);
                out.push(']');
            }
            Expr::Dict(entries) => {
                out.push('{');
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    Expr::Str(key.clone()).render(out);
                    out.push_str(": ");
                    value.render(out);
                }
                out.push('}');
            }
            Expr::Unary(negate, e) => {
                out.push_str(if *negate { "-(" } else { "!(" });
                e.render(out);
                out.push(')');
            }
            Expr::Binary(op, l, r) => {
                out.push('(');
                l.render(out);
                out.push(' ');
                out.push_str(BINARY_OPS[*op]);
                out.push(' ');
                r.render(out);
                out.push(')');
            }
            Expr::Call(call, args) => {
                out.push_str(CALLS[*call].0);
                out.push('(');
                Self::render_list(args, out);
                out.push(')');
            }
            Expr::If(c, t, e) => {
                out.push_str("if (");
                c.render(out);
                out.push_str("): (");
                t.render(out);
                out.push_str(") else: (");
                e.render(out);
                out.push(')');
            }
            Expr::Let(v, value, body) => {
                out.push_str("do let ");
                out.push_str(VARS[*v]);
                out.push_str(" = (");
                value.render(out);
                out.push_str(") | ");
                body.render(out);
                out.push_str(" end");
            }
            Expr::Def(body, rest) => {
                out.push_str("do def f(a, b): ");
                body.render(out);
                out.push_str("; | ");
                rest.render(out);
                out.push_str(" | f(1, 2) end");
            }
            Expr::Lambda(body, arg) => {
                out.push_str("do let g = fn(a): ");
                body.render(out);
                out.push_str("; | g(");
                arg.render(out);
                out.push_str(") end");
            }
            Expr::Foreach(items, body) => {
                out.push_str("foreach(a, [");
                items.render(out);
                out.push_str("]): ");
                body.render(out);
                out.push(';');
            }
            Expr::Match(value, arms, default) => {
                out.push_str("match (");
                value.render(out);
                out.push_str("): ");
                for (ty, body) in arms {
                    out.push_str(&format!("| :{}: (", MATCH_TYPES[*ty]));
                    body.render(out);
                    out.push_str(") ");
                }
                out.push_str("| _: (");
                default.render(out);
                out.push_str(") end");
            }
            Expr::Try(body, handler) => {
                out.push_str("try: (");
                body.render(out);
                out.push_str(") catch: (");
                handler.render(out);
                out.push_str(");");
            }
            Expr::Pipe(l, r) => {
                out.push_str("((");
                l.render(out);
                out.push_str(") | (");
                r.render(out);
                out.push_str("))");
            }
        }
    }

    fn render_list(items: &[Expr], out: &mut String) {
        for (i, item) in items.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            item.render(out);
        }
    }
}

/// A well-formed mq program: bindings for the variable pool, then one generated expression.
#[derive(Debug, Clone)]
pub struct Program {
    expr: Expr,
}

impl Arbitrary<'_> for Program {
    fn arbitrary(u: &mut Unstructured<'_>) -> Result<Self> {
        Ok(Self {
            expr: Expr::generate(u, 0)?,
        })
    }
}

impl Program {
    /// The program source. The variables are bound first so that they are defined.
    pub fn source(&self) -> String {
        let mut source =
            String::from("let a = 1 | let b = \"s\" | let c = [1, 2] | let xs = [3, 1, 2] | let d = {\"k\": 1} | ");
        self.expr.render(&mut source);
        source
    }
}

/// Reads `source` as an mq program and runs it on a small document, once directly and once
/// through the `.mqc` path.
///
/// A generated program is well-formed, so any lexer or parser error is a finding. When both
/// paths finish, they must print the same result.
pub fn run_program(program: &Program) {
    let source = program.source();
    let direct = run(&source, false);
    let compiled = run(&source, true);
    if let (Outcome::Done(direct), Outcome::Done(compiled)) = (&direct, &compiled) {
        assert_eq!(direct, compiled, "direct and compiled runs differ for {source}");
    }
}

enum Outcome {
    Done(Vec<String>),
    Failed,
}

fn run(source: &str, compiled: bool) -> Outcome {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(std::time::Duration::from_millis(100));
    engine.set_max_call_stack_depth(64);
    let input = mq_lang::parse_markdown_input("# a\n\ntext\n").unwrap();

    let result = if compiled {
        let Ok(mqc) = engine.precompile(source, &[]) else {
            return Outcome::Failed;
        };
        let Ok(loaded) = engine.load(&mqc) else {
            return Outcome::Failed;
        };
        engine.eval_compiled(&loaded, input.into_iter())
    } else {
        engine.eval(source, input.into_iter())
    };

    match result {
        Ok(values) => Outcome::Done(values.into_iter().map(|v| v.to_string()).collect()),
        Err(err) => {
            let message = format!("{err:?}");
            let syntax = ["cause: Parse", "cause: Lexer"].iter().any(|m| message.contains(m));
            assert!(!syntax, "generated program is rejected: {source}\n{message}");
            Outcome::Failed
        }
    }
}
