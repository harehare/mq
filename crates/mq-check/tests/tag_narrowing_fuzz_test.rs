//! Generated programs around results written as tagged dicts, such as `{ok: true, value: v}` and
//! `{ok: false, error: e}`.
//!
//! Every program is built so that its types are right or wrong by construction:
//!
//! - well-typed programs must run, and the checker must report nothing (not even an uncertain
//!   error), whatever the way the tag is tested and the way the record reaches the code
//! - a program with one wrong operation in a branch must make the checker report an error when the
//!   checker is expected to follow that branch
//!
//! The generator is deterministic for a given seed. `TAG_FUZZ_COUNT` and `TAG_FUZZ_SEED` run more.

use std::time::Duration;

use mq_check::TypeChecker;
use mq_hir::Hir;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum FieldTy {
    Num,
    Str,
}

#[derive(Clone, Debug)]
enum Tag {
    Bool(bool),
    Str(&'static str),
}

impl Tag {
    fn literal(&self) -> String {
        match self {
            Tag::Bool(b) => b.to_string(),
            Tag::Str(s) => format!("\"{s}\""),
        }
    }
}

#[derive(Clone, Debug)]
struct Variant {
    tag: Tag,
    field: &'static str,
    ty: FieldTy,
}

impl Variant {
    fn value(&self, rng: &mut Rng) -> String {
        match self.ty {
            FieldTy::Num => rng.below(50).to_string(),
            FieldTy::Str => format!("\"{}\"", ["x", "yy", "zzz"][rng.below(3)]),
        }
    }

    /// An operation on the field that its type allows.
    fn good_op(&self, read: &str, rng: &mut Rng) -> String {
        match (self.ty, rng.below(4)) {
            (FieldTy::Num, 0) => format!("{read} + 1"),
            (FieldTy::Num, 1) => format!("{read} * 2"),
            (FieldTy::Num, 2) => format!("abs({read})"),
            (FieldTy::Num, _) => format!("{read} - 1"),
            (FieldTy::Str, 0) => format!("upcase({read})"),
            (FieldTy::Str, 1) => format!("{read} + \"!\""),
            (FieldTy::Str, 2) => format!("len({read})"),
            (FieldTy::Str, _) => format!("downcase({read})"),
        }
    }

    /// An operation on the field that its type does not allow.
    fn bad_op(&self, read: &str, rng: &mut Rng) -> String {
        match (self.ty, rng.below(2)) {
            (FieldTy::Num, 0) => format!("upcase({read})"),
            (FieldTy::Num, _) => format!("trim({read})"),
            (FieldTy::Str, 0) => format!("abs({read})"),
            (FieldTy::Str, _) => format!("{read} - 1"),
        }
    }
}

/// How the record gets to the code that tests its tag.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Source {
    /// `let r = make(flag)`, a union of both variants
    Call,
    /// `use(make(flag))`, where the callee has an unannotated parameter
    CallArgument,
    /// `use({tag: .., field: ..})`, a record literal written in the call
    LiteralArgument,
}

struct Program {
    code: String,
    /// Whether the checker is expected to report an error, and the runtime to fail
    expect_error: bool,
    /// Whether the runtime is expected to fail, when the checker is expected to report an error
    runtime_fails: Option<bool>,
}

struct Gen {
    rng: Rng,
}

impl Gen {
    fn variants(&mut self) -> (&'static str, Variant, Variant) {
        let rng = &mut self.rng;
        let (key, a, b) = if rng.chance(50) {
            let key = ["ok", "success", "is_ok"][rng.below(3)];
            (key, Tag::Bool(true), Tag::Bool(false))
        } else {
            let pairs = [("a", "b"), ("ok", "err"), ("success", "failure"), ("num", "text")];
            let (a, b) = pairs[rng.below(pairs.len())];
            (["kind", "type", "status"][rng.below(3)], Tag::Str(a), Tag::Str(b))
        };
        let ty_a = if rng.chance(50) { FieldTy::Num } else { FieldTy::Str };
        let ty_b = if rng.chance(50) { FieldTy::Num } else { FieldTy::Str };
        // The same field name with different types is the case a tag exists for
        let (field_a, field_b) = if rng.chance(25) {
            ("data", "data")
        } else {
            let names = [
                ("value", "error"),
                ("result", "reason"),
                ("data", "message"),
                ("n", "s"),
            ];
            names[rng.below(names.len())]
        };
        (
            key,
            Variant {
                tag: a,
                field: field_a,
                ty: ty_a,
            },
            Variant {
                tag: b,
                field: field_b,
                ty: ty_b,
            },
        )
    }

    /// A condition that holds for `selected` and not for `other`.
    fn condition(&mut self, key: &str, selected: &Variant, other: &Variant, var: &str) -> String {
        let read = format!("{var}[\"{key}\"]");
        let rng = &mut self.rng;
        match (&selected.tag, &other.tag) {
            (Tag::Bool(true), _) => match rng.below(5) {
                0 => read,
                1 => format!("{read} == true"),
                2 => format!("true == {read}"),
                3 => format!("!({read} == false)"),
                _ => format!("not({read} == false)"),
            },
            (Tag::Bool(false), _) => match rng.below(5) {
                0 => format!("!{read}"),
                1 => format!("{read} == false"),
                2 => format!("{read} != true"),
                3 => format!("not({read})"),
                _ => format!("false == {read}"),
            },
            (Tag::Str(s), Tag::Str(o)) => match rng.below(5) {
                0 => format!("{read} == \"{s}\""),
                1 => format!("\"{s}\" == {read}"),
                2 => format!("{read} != \"{o}\""),
                3 => format!("!({read} == \"{o}\")"),
                _ => format!("not({read} != \"{s}\")"),
            },
            _ => unreachable!("both variants use the same kind of tag"),
        }
    }

    fn body(&mut self, variant: &Variant, var: &str, mutate: bool) -> String {
        let read = format!("{var}[\"{}\"]", variant.field);
        let op = if mutate {
            variant.bad_op(&read, &mut self.rng)
        } else {
            variant.good_op(&read, &mut self.rng)
        };
        if self.rng.chance(50) {
            op
        } else {
            format!("do let t = {op} | t end")
        }
    }

    /// The `if` chain that tests the tag of `var` and reads the field of the variant it selected.
    /// `mutate` is the variant whose branch gets a wrong operation.
    fn chain(&mut self, key: &str, a: &Variant, b: &Variant, var: &str, mutate: Option<usize>) -> String {
        let a_first = self.rng.chance(50);
        let (first, second, first_index) = if a_first { (a, b, 0) } else { (b, a, 1) };
        let cond = self.condition(key, first, second, var);
        let first_body = self.body(first, var, mutate == Some(first_index));
        let second_body = self.body(second, var, mutate == Some(1 - first_index));
        match (&first.tag, self.rng.below(3)) {
            // An elif that tests the other variant, ending in an else that is never taken
            (Tag::Str(_), 0) => {
                let cond2 = self.condition(key, second, first, var);
                format!("if ({cond}): {first_body} elif ({cond2}): {second_body} else: 0")
            }
            _ => format!("if ({cond}): {first_body} else: {second_body}"),
        }
    }

    fn program(&mut self, mutate: bool) -> Program {
        let (key, a, b) = self.variants();
        let source = [Source::Call, Source::CallArgument, Source::LiteralArgument][self.rng.below(3)];
        let mutated = mutate.then(|| self.rng.below(2));
        let variant_of = |i: usize| if i == 0 { &a } else { &b };

        let make = format!(
            "def make(flag): if (flag): {{{key}: {}, {}: {}}} else: {{{key}: {}, {}: {}}} end",
            a.tag.literal(),
            a.field,
            a.value(&mut self.rng),
            b.tag.literal(),
            b.field,
            b.value(&mut self.rng),
        );
        let flag = if self.rng.chance(50) { "true" } else { "false" };
        let selected = if flag == "true" { 0 } else { 1 };
        let var = if source == Source::Call { "r" } else { "x" };
        let chain = self.chain(key, &a, &b, var, mutated);

        let (code, expect_error, runtime_fails) = match source {
            Source::Call => (
                format!("{make}\nlet r = make({flag})\n| {chain}"),
                mutated.is_some(),
                mutated.map(|m| m == selected),
            ),
            Source::CallArgument => (
                format!("{make}\ndef use(x): {chain} end\nuse(make({flag}))"),
                // The callee is not followed through the union that `make` returns
                false,
                mutated.map(|m| m == selected),
            ),
            Source::LiteralArgument => {
                let chosen = variant_of(selected);
                let literal = format!("{{{key}: {}, {}: {}}}", chosen.tag.literal(), chosen.field, {
                    chosen.value(&mut self.rng)
                });
                (
                    format!("def use(x): {chain} end\nuse({literal})"),
                    mutated == Some(selected),
                    Some(mutated == Some(selected)),
                )
            }
        };
        Program {
            code,
            expect_error,
            runtime_fails,
        }
    }
}

fn check(code: &str) -> Vec<String> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    TypeChecker::new().check(&hir).iter().map(ToString::to_string).collect()
}

fn runs(engine: &mut mq_lang::DefaultEngine, code: &str) -> Result<(), String> {
    engine
        .eval(code, mq_lang::null_input().into_iter())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn env(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn engine() -> mq_lang::DefaultEngine {
    let mut engine = mq_lang::DefaultEngine::default();
    engine.load_builtin_module();
    engine.set_timeout(Duration::from_millis(500));
    engine
}

#[test]
fn well_typed_tagged_records_are_not_reported() {
    let mut generator = Gen {
        rng: Rng(env("TAG_FUZZ_SEED", 0x7a9) | 1),
    };
    let mut engine = engine();
    let mut problems = Vec::new();
    for _ in 0..env("TAG_FUZZ_COUNT", 150) {
        let program = generator.program(false);
        if let Err(error) = runs(&mut engine, &program.code) {
            problems.push(format!("generator bug, the program fails: {error}\n{}", program.code));
            continue;
        }
        let errors = check(&program.code);
        if !errors.is_empty() {
            problems.push(format!("false positive: {}\n{}", errors[0], program.code));
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems\n{}",
        problems.len(),
        problems.iter().take(8).cloned().collect::<Vec<_>>().join("\n---\n")
    );
}

#[test]
fn a_wrong_operation_in_a_followed_branch_is_reported() {
    let mut generator = Gen {
        rng: Rng(env("TAG_FUZZ_SEED", 0x7a9).wrapping_add(1) | 1),
    };
    let mut engine = engine();
    let mut problems = Vec::new();
    for _ in 0..env("TAG_FUZZ_COUNT", 150) {
        let program = generator.program(true);
        let errors = check(&program.code);
        if program.expect_error && errors.is_empty() {
            problems.push(format!("missed error\n{}", program.code));
        }
        if !program.expect_error && program.runtime_fails == Some(false) && !errors.is_empty() {
            problems.push(format!("false positive: {}\n{}", errors[0], program.code));
        }
        if let Some(fails) = program.runtime_fails {
            let failed = runs(&mut engine, &program.code).is_err();
            if failed != fails {
                problems.push(format!("generator bug, runtime failed={failed}\n{}", program.code));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems\n{}",
        problems.len(),
        problems.iter().take(8).cloned().collect::<Vec<_>>().join("\n---\n")
    );
}
