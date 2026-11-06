//! A small sort-checked SMT-LIB term language. Terms are only built through [`Typed`]'s
//! constructors, which reject ill-sorted combinations, so a rendered script is well-sorted.

use std::fmt;

#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) enum Sort {
    Int,
    Bool,
    Str,
    /// An array, modeled as an SMT-LIB sequence.
    Seq(Box<Sort>),
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sort::Int => f.write_str("Int"),
            Sort::Bool => f.write_str("Bool"),
            Sort::Str => f.write_str("String"),
            Sort::Seq(elem) => write!(f, "(Seq {elem})"),
        }
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
    Str(String),
    /// `(as seq.empty (Seq elem))`.
    EmptySeq(Sort),
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
            Term::Str(text) => write!(f, "\"{}\"", text.replace('"', "\"\"")),
            Term::EmptySeq(elem) => write!(f, "(as seq.empty (Seq {elem}))"),
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
    pub(super) fn expect(mut self, expected: Sort, context: &str) -> Result<Self> {
        // `[]` has no item sort of its own, so it takes the one expected here.
        if let (Term::EmptySeq(_), Sort::Seq(elem)) = (&self.term, &expected) {
            self.term = Term::EmptySeq((**elem).clone());
            self.sort = expected.clone();
        }
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

    /// `a + b` over two Ints, two strings (concatenation) or two arrays (concatenation).
    pub(super) fn add(a: Typed, b: Typed) -> Result<Self> {
        match &a.sort {
            Sort::Int => Self::arith("+", a, b),
            Sort::Str => {
                let b = b.expect(Sort::Str, "+")?;
                Ok(Self::app("str.++", vec![a, b], Sort::Str))
            }
            Sort::Seq(_) => {
                let sort = a.sort.clone();
                let b = b.expect(sort.clone(), "+")?;
                Ok(Self::app("seq.++", vec![a, b], sort))
            }
            Sort::Bool => Err(SmtError::Mismatch {
                context: "+".to_string(),
                expected: Sort::Int,
                found: Sort::Bool,
            }),
        }
    }

    /// A string literal. Only printable ASCII is translated.
    pub(super) fn string(text: &str) -> Result<Self> {
        if !text.chars().all(|c| (' '..='~').contains(&c)) {
            return Err(SmtError::Unsupported(
                "non-ASCII or control characters in a string".to_string(),
            ));
        }
        Ok(Self {
            term: Term::Str(text.to_string()),
            sort: Sort::Str,
        })
    }

    /// An array literal whose items share one sort (an empty one is a sequence of Int).
    pub(super) fn seq(items: Vec<Typed>) -> Result<Self> {
        let Some(first) = items.first() else {
            return Ok(Self {
                term: Term::EmptySeq(Sort::Int),
                sort: Sort::Seq(Box::new(Sort::Int)),
            });
        };
        let elem = first.sort.clone();
        let units = items
            .into_iter()
            .map(|i| {
                i.expect(elem.clone(), "array item")
                    .map(|i| Term::App("seq.unit", vec![i.term]))
            })
            .collect::<Result<Vec<_>>>()?;
        let term = units
            .into_iter()
            .reduce(|acc, u| Term::App("seq.++", vec![acc, u]))
            .unwrap_or(Term::EmptySeq(elem.clone()));
        Ok(Self {
            term,
            sort: Sort::Seq(Box::new(elem)),
        })
    }

    /// `len(x)` of a string or an array.
    pub(super) fn len(a: Typed) -> Result<Self> {
        let op = match a.sort {
            Sort::Str => "str.len",
            Sort::Seq(_) => "seq.len",
            _ => {
                return Err(SmtError::Mismatch {
                    context: "len".to_string(),
                    expected: Sort::Str,
                    found: a.sort,
                });
            }
        };
        Ok(Self::app(op, vec![a], Sort::Int))
    }

    /// `a[i]` for an array (its item) or a string (the one-character string).
    pub(super) fn index(a: Typed, i: Typed) -> Result<Self> {
        let i = i.expect(Sort::Int, "index")?;
        match a.sort.clone() {
            Sort::Str => Ok(Self::app("str.at", vec![a, i], Sort::Str)),
            Sort::Seq(elem) => Ok(Self::app("seq.nth", vec![a, i], *elem)),
            found => Err(SmtError::Mismatch {
                context: "index".to_string(),
                expected: Sort::Str,
                found,
            }),
        }
    }

    /// `contains`, `starts_with` or `ends_with` of a string and a second string.
    pub(super) fn str_predicate(name: &str, subject: Typed, other: Typed) -> Result<Self> {
        let (subject, other) = (subject.expect(Sort::Str, name)?, other.expect(Sort::Str, name)?);
        Ok(match name {
            "contains" => Self::app("str.contains", vec![subject, other], Sort::Bool),
            "starts_with" => Self::app("str.prefixof", vec![other, subject], Sort::Bool),
            _ => Self::app("str.suffixof", vec![other, subject], Sort::Bool),
        })
    }

    /// `a % b` as the truncated remainder (the sign follows `a`), which is what mq computes on
    /// integers. A zero divisor is left unconstrained: mq raises an error there.
    pub(super) fn rem(a: Typed, b: Typed) -> Result<Self> {
        let (a, b) = (a.expect(Sort::Int, "%")?, b.expect(Sort::Int, "%")?);
        let abs_b = Self::ite(Self::compare(">=", b.clone(), Self::int(0))?, b.clone(), Self::neg(b)?)?;
        let positive = Self::arith("mod", a.clone(), abs_b.clone())?;
        let negative = Self::neg(Self::arith("mod", Self::neg(a.clone())?, abs_b)?)?;
        Self::ite(Self::compare(">=", a, Self::int(0))?, positive, negative)
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
        let b = b.expect(a.sort.clone(), if negate { "!=" } else { "==" })?;
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
        let otherwise = otherwise.expect(then.sort.clone(), "if branches")?;
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
            .map(|(a, s)| a.expect(s.clone(), name).map(|a| a.term))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            term: Term::Call(name.to_string(), terms),
            sort: sig.ret.clone(),
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
