//! A small sort-checked SMT-LIB term language. Terms are only built through [`Typed`]'s
//! constructors, which reject ill-sorted combinations, so a rendered script is well-sorted.

use std::fmt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Sort {
    Int,
    Bool,
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sort::Int => "Int",
            Sort::Bool => "Bool",
        })
    }
}

/// Why an mq expression could not be translated.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum SmtError {
    /// A construct outside the supported subset.
    Unsupported(String),
    /// An operand has the wrong sort.
    Mismatch {
        context: String,
        expected: Sort,
        found: Sort,
    },
    /// Any other problem, e.g. an unknown variable.
    Invalid(String),
}

impl fmt::Display for SmtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmtError::Unsupported(what) => write!(f, "unsupported {what}"),
            SmtError::Mismatch {
                context,
                expected,
                found,
            } => write!(f, "{context}: expected {expected}, found {found}"),
            SmtError::Invalid(why) => f.write_str(why),
        }
    }
}

pub(super) type Result<T> = std::result::Result<T, SmtError>;

/// Words that must be quoted to be used as SMT-LIB symbols.
const RESERVED: &[&str] = &[
    "and", "or", "not", "ite", "let", "true", "false", "mod", "div", "abs", "assert", "echo", "forall", "exists",
];

/// Quotes `name` with `|...|` when it is not a plain SMT-LIB symbol.
pub(super) fn symbol(name: &str) -> String {
    let simple = name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && !RESERVED.contains(&name);
    if simple { name.to_string() } else { format!("|{name}|") }
}

#[derive(Clone, Debug)]
pub(super) enum Term {
    Int(i64),
    Bool(bool),
    Var(String),
    /// A built-in operator applied to arguments.
    App(&'static str, Vec<Term>),
    /// A call of a user function defined in the script.
    Call(String, Vec<Term>),
    Ite(Box<Term>, Box<Term>, Box<Term>),
    Let(String, Box<Term>, Box<Term>),
}

impl fmt::Display for Term {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Term::Int(n) if *n < 0 => write!(f, "(- {})", n.unsigned_abs()),
            Term::Int(n) => write!(f, "{n}"),
            Term::Bool(b) => write!(f, "{b}"),
            Term::Var(name) => f.write_str(&symbol(name)),
            Term::App(op, args) => write_app(f, op, args),
            Term::Call(name, args) if args.is_empty() => f.write_str(&symbol(name)),
            Term::Call(name, args) => write_app(f, &symbol(name), args),
            Term::Ite(c, t, e) => write!(f, "(ite {c} {t} {e})"),
            Term::Let(name, value, body) => write!(f, "(let (({} {value})) {body})", symbol(name)),
        }
    }
}

fn write_app(f: &mut fmt::Formatter<'_>, op: &str, args: &[Term]) -> fmt::Result {
    write!(f, "({op}")?;
    for a in args {
        write!(f, " {a}")?;
    }
    f.write_str(")")
}

/// The sorts of a function's parameters and result.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Signature {
    pub(super) params: Vec<Sort>,
    pub(super) ret: Sort,
}

/// A term together with its sort.
#[derive(Clone, Debug)]
pub(super) struct Typed {
    pub(super) term: Term,
    pub(super) sort: Sort,
}

impl Typed {
    pub(super) fn int(n: i64) -> Self {
        Self {
            term: Term::Int(n),
            sort: Sort::Int,
        }
    }

    pub(super) fn bool(b: bool) -> Self {
        Self {
            term: Term::Bool(b),
            sort: Sort::Bool,
        }
    }

    pub(super) fn var(name: &str, sort: Sort) -> Self {
        Self {
            term: Term::Var(name.to_string()),
            sort,
        }
    }

    /// Checks that this term has sort `expected`; `context` names the construct for the error.
    pub(super) fn expect(self, expected: Sort, context: &str) -> Result<Self> {
        if self.sort == expected {
            Ok(self)
        } else {
            Err(SmtError::Mismatch {
                context: context.to_string(),
                expected,
                found: self.sort,
            })
        }
    }

    fn app(op: &'static str, args: Vec<Typed>, sort: Sort) -> Self {
        Self {
            term: Term::App(op, args.into_iter().map(|a| a.term).collect()),
            sort,
        }
    }

    /// `a op b` over Ints giving an Int (`+`, `-`, `*`).
    pub(super) fn arith(op: &'static str, a: Typed, b: Typed) -> Result<Self> {
        Ok(Self::app(
            op,
            vec![a.expect(Sort::Int, op)?, b.expect(Sort::Int, op)?],
            Sort::Int,
        ))
    }

    /// `a op b` over Ints giving a Bool (`<`, `<=`, `>`, `>=`).
    pub(super) fn compare(op: &'static str, a: Typed, b: Typed) -> Result<Self> {
        Ok(Self::app(
            op,
            vec![a.expect(Sort::Int, op)?, b.expect(Sort::Int, op)?],
            Sort::Bool,
        ))
    }

    /// `a == b` (or `a != b`) over two terms of the same sort.
    pub(super) fn equal(a: Typed, b: Typed, negate: bool) -> Result<Self> {
        let b = b.expect(a.sort, if negate { "!=" } else { "==" })?;
        let eq = Self::app("=", vec![a, b], Sort::Bool);
        Ok(if negate { Self::not(eq) } else { eq })
    }

    /// `and` / `or` over Bool operands.
    pub(super) fn connective(op: &'static str, args: Vec<Typed>) -> Result<Self> {
        let args = args
            .into_iter()
            .map(|a| a.expect(Sort::Bool, op))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self::app(op, args, Sort::Bool))
    }

    /// Logical negation of a Bool term.
    pub(super) fn not(a: Typed) -> Self {
        debug_assert_eq!(a.sort, Sort::Bool);
        Self::app("not", vec![a], Sort::Bool)
    }

    pub(super) fn neg(a: Typed) -> Result<Self> {
        Ok(Self::app("-", vec![a.expect(Sort::Int, "-")?], Sort::Int))
    }

    pub(super) fn ite(cond: Typed, then: Typed, otherwise: Typed) -> Result<Self> {
        let cond = cond.expect(Sort::Bool, "if condition")?;
        let otherwise = otherwise.expect(then.sort, "if branches")?;
        Ok(Self {
            sort: then.sort,
            term: Term::Ite(Box::new(cond.term), Box::new(then.term), Box::new(otherwise.term)),
        })
    }

    /// Calls `name` with `args`, which must match `sig`.
    pub(super) fn call(name: &str, sig: &Signature, args: Vec<Typed>) -> Result<Self> {
        if args.len() != sig.params.len() {
            return Err(SmtError::Invalid(format!(
                "`{name}` takes {} arguments, got {}",
                sig.params.len(),
                args.len()
            )));
        }
        let terms = args
            .into_iter()
            .zip(&sig.params)
            .map(|(a, s)| a.expect(*s, name).map(|a| a.term))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            term: Term::Call(name.to_string(), terms),
            sort: sig.ret,
        })
    }

    /// `(let ((name value)) self)`.
    pub(super) fn bind(self, name: &str, value: Typed) -> Self {
        Self {
            sort: self.sort,
            term: Term::Let(name.to_string(), Box::new(value.term), Box::new(self.term)),
        }
    }
}

/// A `define-fun` / `define-fun-rec` command.
pub(super) struct Define {
    pub(super) name: String,
    pub(super) params: Vec<(String, Sort)>,
    pub(super) ret: Sort,
    pub(super) body: Term,
    pub(super) recursive: bool,
}

impl fmt::Display for Define {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|(p, s)| format!("({} {s})", symbol(p)))
            .collect();
        write!(
            f,
            "({} {} ({}) {} {})",
            if self.recursive { "define-fun-rec" } else { "define-fun" },
            symbol(&self.name),
            params.join(" "),
            self.ret,
            self.body
        )
    }
}
