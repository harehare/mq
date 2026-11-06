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
    /// A value or `None`, modeled as the datatype `(Option T)`.
    Opt(Box<Sort>),
    /// A dict with known keys, modeled as a datatype. Fields are sorted by name.
    Record(Vec<(String, Sort)>),
}

impl Sort {
    /// A record sort; its fields are sorted by name so equal shapes compare equal.
    pub(super) fn record(mut fields: Vec<(String, Sort)>) -> Sort {
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        Sort::Record(fields)
    }

    /// The datatype name of a record, built from its fields so equal shapes share it.
    fn record_name(fields: &[(String, Sort)]) -> String {
        fields.iter().fold(String::from("R"), |name, (field, sort)| {
            format!("{name}_{field}_{}", sort.mangled())
        })
    }

    fn mangled(&self) -> String {
        match self {
            Sort::Int => "Int".to_string(),
            Sort::Bool => "Bool".to_string(),
            Sort::Str => "String".to_string(),
            Sort::Seq(elem) => format!("Seq_{}", elem.mangled()),
            Sort::Opt(inner) => format!("Opt_{}", inner.mangled()),
            Sort::Record(fields) => Self::record_name(fields),
        }
    }

    /// The accessor of `field` of a record sort.
    pub(super) fn accessor(fields: &[(String, Sort)], field: &str) -> String {
        format!("{}.{field}", Self::record_name(fields))
    }

    /// The constructor of a record sort.
    pub(super) fn constructor(fields: &[(String, Sort)]) -> String {
        format!("mk_{}", Self::record_name(fields))
    }

    /// Every record `self` needs, inner ones first, without repeats.
    fn collect_datatypes(&self, out: &mut Vec<Sort>) {
        match self {
            Sort::Seq(inner) | Sort::Opt(inner) => inner.collect_datatypes(out),
            Sort::Record(fields) => {
                fields.iter().for_each(|(_, s)| s.collect_datatypes(out));
                if !out.contains(self) {
                    out.push(self.clone());
                }
            }
            _ => {}
        }
    }

    fn uses_option(&self) -> bool {
        match self {
            Sort::Opt(_) => true,
            Sort::Seq(inner) => inner.uses_option(),
            Sort::Record(fields) => fields.iter().any(|(_, s)| s.uses_option()),
            _ => false,
        }
    }
}

/// The `declare-datatype` commands the given sorts need (the generic `Option` and each record).
pub(super) fn datatype_declarations(sorts: &[Sort]) -> String {
    let mut records = Vec::new();
    sorts.iter().for_each(|s| s.collect_datatypes(&mut records));
    let mut out = String::new();
    if sorts.iter().any(Sort::uses_option) {
        out.push_str("(declare-datatype Option (par (T) ((none) (some (val T)))))\n");
    }
    for record in &records {
        let Sort::Record(fields) = record else {
            continue;
        };
        let accessors: Vec<String> = fields
            .iter()
            .map(|(field, sort)| format!("({} {sort})", symbol(&Sort::accessor(fields, field))))
            .collect();
        out.push_str(&format!(
            "(declare-datatype {} (({} {})))\n",
            symbol(&Sort::record_name(fields)),
            symbol(&Sort::constructor(fields)),
            accessors.join(" ")
        ));
    }
    out
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Sort::Int => f.write_str("Int"),
            Sort::Bool => f.write_str("Bool"),
            Sort::Str => f.write_str("String"),
            Sort::Seq(elem) => write!(f, "(Seq {elem})"),
            Sort::Opt(inner) => write!(f, "(Option {inner})"),
            Sort::Record(fields) => f.write_str(&symbol(&Sort::record_name(fields))),
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
    /// `(as none (Option T))` for the item sort `T`.
    NoneOf(Sort),
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
            Term::NoneOf(inner) => write!(f, "(as none (Option {inner}))"),
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
        self.adopt(&expected);
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

    /// Fits `self` to `target`: `[]` and `None` take its item sort, and a plain value becomes the
    /// option holding it, since mq does not tell a value from an optional one.
    fn adopt(&mut self, target: &Sort) {
        match (&self.term, target) {
            (Term::EmptySeq(_), Sort::Seq(elem)) => self.term = Term::EmptySeq((**elem).clone()),
            (Term::NoneOf(_), Sort::Opt(inner)) => self.term = Term::NoneOf((**inner).clone()),
            (_, Sort::Opt(inner)) if self.sort == **inner => {
                self.term = Term::App("some", vec![self.term.clone()]);
            }
            _ => return,
        }
        self.sort = target.clone();
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
            found => Err(SmtError::Mismatch {
                context: "+".to_string(),
                expected: Sort::Int,
                found: found.clone(),
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

    /// `a[i]` for an array or a string: the item (or one-character string), or `None` when the
    /// index is out of range. A negative index counts from the end, as in mq.
    pub(super) fn index(a: Typed, i: Typed) -> Result<Self> {
        let i = i.expect(Sort::Int, "index")?;
        let (len_op, at_op, item) = match a.sort.clone() {
            Sort::Str => ("str.len", "str.at", Sort::Str),
            Sort::Seq(elem) => ("seq.len", "seq.nth", *elem),
            found => {
                return Err(SmtError::Mismatch {
                    context: "index".to_string(),
                    expected: Sort::Str,
                    found,
                });
            }
        };
        let app = |op: &'static str, args: Vec<Term>| Term::App(op, args);
        let len = app(len_op, vec![a.term.clone()]);
        let effective = Term::Ite(
            Box::new(app(">=", vec![i.term.clone(), Term::Int(0)])),
            Box::new(i.term.clone()),
            Box::new(app("+", vec![len.clone(), i.term])),
        );
        let in_range = app(
            "and",
            vec![
                app(">=", vec![effective.clone(), Term::Int(0)]),
                app("<", vec![effective.clone(), len]),
            ],
        );
        let value = app("some", vec![app(at_op, vec![a.term, effective])]);
        Ok(Self {
            term: Term::Ite(
                Box::new(in_range),
                Box::new(value),
                Box::new(Term::NoneOf(item.clone())),
            ),
            sort: Sort::Opt(Box::new(item)),
        })
    }

    /// The `None` literal; it takes its item sort from what it is compared or merged with.
    pub(super) fn none() -> Self {
        Self {
            term: Term::NoneOf(Sort::Int),
            sort: Sort::Opt(Box::new(Sort::Int)),
        }
    }

    /// `is_none(x)`: a plain value is never `None`.
    pub(super) fn is_none(a: Typed) -> Self {
        match a.sort.clone() {
            Sort::Opt(inner) => Self::app(
                "=",
                vec![
                    a,
                    Self {
                        term: Term::NoneOf(*inner.clone()),
                        sort: Sort::Opt(inner),
                    },
                ],
                Sort::Bool,
            ),
            _ => Self::bool(false),
        }
    }

    /// A type predicate such as `is_number(x)`: whether `x`'s sort satisfies `test`. For an
    /// optional value that is whether it is not `None` and its item satisfies `test`.
    pub(super) fn kind_test(a: Typed, test: impl Fn(&Sort) -> bool) -> Self {
        match &a.sort {
            Sort::Opt(inner) if test(inner) => Self::not(Self::is_none(a)),
            Sort::Opt(_) => Self::bool(false),
            sort => Self::bool(test(sort)),
        }
    }

    /// The value inside a non-`None` option.
    pub(super) fn unwrap(a: Typed) -> Result<Self> {
        match a.sort.clone() {
            Sort::Opt(inner) => Ok(Self {
                term: Term::App("val", vec![a.term]),
                sort: *inner,
            }),
            found => Err(SmtError::Mismatch {
                context: "value".to_string(),
                expected: Sort::Opt(Box::new(found.clone())),
                found,
            }),
        }
    }

    /// `d[:key]` or `d["key"]` on a dict of known shape.
    pub(super) fn field(rec: Typed, key: &str) -> Result<Self> {
        let Sort::Record(fields) = rec.sort.clone() else {
            return Err(SmtError::Unsupported(format!(
                "key `{key}` on a value that is not a dict of known shape"
            )));
        };
        let (_, sort) = fields.iter().find(|(name, _)| name == key).ok_or_else(|| {
            SmtError::Unsupported(format!("key `{key}` that the dict's declared shape does not have"))
        })?;
        Ok(Self {
            term: Term::Call(Sort::accessor(&fields, key), vec![rec.term]),
            sort: sort.clone(),
        })
    }

    /// A dict literal; its shape is the keys written.
    pub(super) fn record(fields: Vec<(String, Typed)>) -> Self {
        let sorts = fields.iter().map(|(k, v)| (k.clone(), v.sort.clone())).collect();
        let sort = Sort::record(sorts);
        let Sort::Record(ordered) = &sort else {
            unreachable!("Sort::record returns a record")
        };
        let terms = ordered
            .iter()
            .filter_map(|(k, _)| fields.iter().find(|(name, _)| name == k).map(|(_, v)| v.term.clone()))
            .collect();
        Self {
            term: Term::Call(Sort::constructor(ordered), terms),
            sort,
        }
    }

    /// `set(d, key, value)` on a dict of known shape.
    pub(super) fn with_field(rec: Typed, key: &str, value: Typed) -> Result<Self> {
        let Sort::Record(fields) = rec.sort.clone() else {
            return Err(SmtError::Unsupported(format!(
                "key `{key}` on a value that is not a dict of known shape"
            )));
        };
        let (_, want) = fields.iter().find(|(name, _)| name == key).ok_or_else(|| {
            SmtError::Unsupported(format!("key `{key}` that the dict's declared shape does not have"))
        })?;
        let value = value.expect(want.clone(), "set")?;
        let terms = fields
            .iter()
            .map(|(name, _)| {
                if name == key {
                    value.term.clone()
                } else {
                    Term::Call(Sort::accessor(&fields, name), vec![rec.term.clone()])
                }
            })
            .collect();
        Ok(Self {
            term: Term::Call(Sort::constructor(&fields), terms),
            sort: rec.sort,
        })
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
    pub(super) fn equal(mut a: Typed, mut b: Typed, negate: bool) -> Result<Self> {
        a.adopt(&b.sort.clone());
        b.adopt(&a.sort.clone());
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

    pub(super) fn ite(cond: Typed, mut then: Typed, mut otherwise: Typed) -> Result<Self> {
        then.adopt(&otherwise.sort.clone());
        otherwise.adopt(&then.sort.clone());
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
