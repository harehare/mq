//! Fuzz-style property tests: a corrupted `.mqc` file must be rejected or run without panicking.
//!
//! The VM trusts verified bytecode, and debug builds turn an out-of-bounds unchecked access
//! into a panic, so these tests catch gaps in the bytecode verifier.
use super::tests::{compile, engine, load, markdown, rewrite};
use super::*;
use crate::tarn::bytecode::{Chunk, OpCode, ParamBinding};
use proptest::prelude::*;
use proptest::sample::Index;
use std::collections::BTreeSet;

/// Programs that together compile to every instruction the VM runs.
const FUZZ_PROGRAMS: &[&str] = &[
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

/// Instructions no compiled program contains, grafted into the fuzz programs by hand.
fn uncompiled_instructions() -> Vec<OpCode> {
    // The peephole pass always fuses this with the jump after it.
    (0..4).map(OpCode::ForeachCollect).collect()
}

/// Every instruction of the fuzz programs, to graft into other programs.
fn donor_instructions() -> Vec<OpCode> {
    thread_local! {
        static DONORS: Vec<OpCode> = FUZZ_PROGRAMS
            .iter()
            .flat_map(|program| {
                let mut ops = Vec::new();
                edit_parts(&compile(program), |parts| {
                    ops.extend(parts.iter().flat_map(|chunks| chunks.iter()).flat_map(|chunk| chunk.code.clone()));
                });
                ops
            })
            .chain(uncompiled_instructions())
            .collect();
    }
    DONORS.with(Clone::clone)
}

/// Rewrites a valid file's decoded chunks, the per-input program first and then the part
/// after `nodes`, keeping its source spans.
fn edit_parts(bytes: &[u8], edit: impl FnOnce(&mut [&mut [Chunk]])) -> Vec<u8> {
    rewrite(bytes, |sections| {
        let source = sections.iter().find(|section| section.tag == SOURCE).unwrap();
        let (_, spans) = decode_source(&source.payload).unwrap();
        let arena = Shared::clone(&engine().token_arena);
        let placeholder = Shared::new(Token {
            range: Range::default(),
            kind: TokenKind::Eof,
            module_id: crate::Module::TOP_LEVEL_MODULE_ID,
        });
        let tokens = spans
            .iter()
            .map(|_| crate::token_alloc(&arena, &placeholder))
            .collect::<Vec<_>>();
        let code = sections.iter_mut().find(|section| section.tag == CODE).unwrap();
        let mut split = code::decode(&code.payload, &tokens, arena).unwrap();
        let mut parts = vec![Shared::get_mut(&mut split.program.chunks).unwrap().as_mut_slice()];
        if let Some(after) = &mut split.after {
            parts.push(Shared::get_mut(&mut after.chunks).unwrap().as_mut_slice());
        }
        edit(&mut parts);
        code.payload = Cow::Owned(code::encode(&split).unwrap().payload);
    })
}

#[test]
fn test_fuzz_programs_cover_every_instruction() {
    let covered = donor_instructions()
        .iter()
        .map(code::instruction_id)
        .collect::<BTreeSet<_>>();
    let missing = code::known_instruction_ids()
        .into_iter()
        .filter(|id| !covered.contains(id))
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "no fuzz program compiles to instruction ids {missing:?}"
    );
}

/// A structural edit to decoded bytecode, placed with indexes into the program.
#[derive(Debug, Clone)]
enum CodeMutation {
    /// Overwrites an instruction with a copy of another one, possibly from another chunk.
    Splice(Index, Index),
    /// Overwrites an instruction with one from another fuzz program.
    Graft(Index, Index),
    Swap(Index, Index),
    Remove(Index),
    Duplicate(Index),
    /// Sets one local slot operand of an instruction.
    Slot(Index, Index, u16),
    /// Grows or shrinks a chunk's locals.
    LocalCount(Index, u16),
    /// Replaces a chunk's code with another chunk's.
    CopyCode(Index, Index),
    /// Sets the slot of one parameter.
    ParamSlot(Index, Index, u16),
    /// Drops a chunk's last parameter.
    DropParam(Index),
    /// Adds a required parameter to a chunk.
    AddParam(Index),
    /// Grows or shrinks a chunk's captured values.
    UpvalueCount(Index, u16),
    SwapConstants(Index, Index, Index),
    ToggleGenerator(Index),
}

fn code_mutation() -> impl Strategy<Value = CodeMutation> {
    prop_oneof![
        (any::<Index>(), any::<Index>()).prop_map(|(from, to)| CodeMutation::Splice(from, to)),
        (any::<Index>(), any::<Index>()).prop_map(|(from, to)| CodeMutation::Graft(from, to)),
        (any::<Index>(), any::<Index>()).prop_map(|(a, b)| CodeMutation::Swap(a, b)),
        any::<Index>().prop_map(CodeMutation::Remove),
        any::<Index>().prop_map(CodeMutation::Duplicate),
        (any::<Index>(), any::<Index>(), 0u16..8).prop_map(|(op, slot, value)| CodeMutation::Slot(op, slot, value)),
        (any::<Index>(), 0u16..8).prop_map(|(chunk, count)| CodeMutation::LocalCount(chunk, count)),
        (any::<Index>(), any::<Index>()).prop_map(|(from, to)| CodeMutation::CopyCode(from, to)),
        (any::<Index>(), any::<Index>(), 0u16..8)
            .prop_map(|(chunk, param, slot)| CodeMutation::ParamSlot(chunk, param, slot)),
        any::<Index>().prop_map(CodeMutation::DropParam),
        any::<Index>().prop_map(CodeMutation::AddParam),
        (any::<Index>(), 0u16..4).prop_map(|(chunk, count)| CodeMutation::UpvalueCount(chunk, count)),
        (any::<Index>(), any::<Index>(), any::<Index>())
            .prop_map(|(chunk, a, b)| CodeMutation::SwapConstants(chunk, a, b)),
        any::<Index>().prop_map(CodeMutation::ToggleGenerator),
    ]
}

/// The `(chunk, pc)` an index picks among every instruction of `chunks`.
fn instruction_at(chunks: &[Chunk], index: &Index) -> (usize, usize) {
    let total = chunks.iter().map(|chunk| chunk.code.len()).sum::<usize>();
    let mut pc = index.index(total.max(1));
    for (chunk_index, chunk) in chunks.iter().enumerate() {
        if pc < chunk.code.len() {
            return (chunk_index, pc);
        }
        pc -= chunk.code.len();
    }
    (0, 0)
}

fn apply_code_mutation(chunks: &mut [Chunk], mutation: &CodeMutation) {
    let chunk_at = |index: &Index| index.index(chunks.len());
    match mutation {
        CodeMutation::Splice(from, to) => {
            let (from_chunk, from_pc) = instruction_at(chunks, from);
            let (to_chunk, to_pc) = instruction_at(chunks, to);
            chunks[to_chunk].code[to_pc] = chunks[from_chunk].code[from_pc].clone();
        }
        CodeMutation::Graft(donor, to) => {
            let donors = donor_instructions();
            let (to_chunk, to_pc) = instruction_at(chunks, to);
            chunks[to_chunk].code[to_pc] = donors[donor.index(donors.len())].clone();
        }
        CodeMutation::Swap(a, b) => {
            let (a_chunk, a_pc) = instruction_at(chunks, a);
            let (b_chunk, b_pc) = instruction_at(chunks, b);
            let a_op = chunks[a_chunk].code[a_pc].clone();
            chunks[a_chunk].code[a_pc] = std::mem::replace(&mut chunks[b_chunk].code[b_pc], a_op);
        }
        CodeMutation::Remove(index) => {
            let (chunk, pc) = instruction_at(chunks, index);
            if chunks[chunk].code.len() > 1 {
                chunks[chunk].code.remove(pc);
            }
        }
        CodeMutation::Duplicate(index) => {
            let (chunk, pc) = instruction_at(chunks, index);
            let op = chunks[chunk].code[pc].clone();
            chunks[chunk].code.insert(pc, op);
        }
        CodeMutation::Slot(index, slot, value) => {
            let (chunk, pc) = instruction_at(chunks, index);
            let mut slots = 0;
            chunks[chunk].code[pc].for_each_local_slot_mut(|_| slots += 1);
            if slots > 0 {
                let target = slot.index(slots);
                let mut seen = 0;
                chunks[chunk].code[pc].for_each_local_slot_mut(|slot| {
                    if seen == target {
                        *slot = *value;
                    }
                    seen += 1;
                });
            }
        }
        CodeMutation::LocalCount(index, count) => {
            let chunk = &mut chunks[chunk_at(index)];
            chunk.local_names.resize(usize::from(*count), Ident::new("fuzz"));
            chunk.local_mutable.resize(usize::from(*count), true);
            chunk.local_count = *count;
        }
        CodeMutation::CopyCode(from, to) => {
            let code = chunks[chunk_at(from)].code.clone();
            chunks[chunk_at(to)].code = code;
        }
        CodeMutation::ParamSlot(index, param, value) => {
            let bindings = &mut chunks[chunk_at(index)].param_shape.bindings;
            if !bindings.is_empty() {
                let binding = param.index(bindings.len());
                match &mut bindings[binding] {
                    ParamBinding::Required(slot) | ParamBinding::Optional(slot, ..) | ParamBinding::Variadic(slot) => {
                        *slot = *value;
                    }
                }
            }
        }
        CodeMutation::DropParam(index) => {
            let shape = &mut chunks[chunk_at(index)].param_shape;
            match shape.bindings.pop() {
                Some(ParamBinding::Required(_)) => shape.required -= 1,
                Some(ParamBinding::Variadic(_)) => shape.has_variadic = false,
                _ => {}
            }
        }
        CodeMutation::AddParam(index) => {
            let shape = &mut chunks[chunk_at(index)].param_shape;
            if shape.bindings.len() == shape.required {
                shape.bindings.push(ParamBinding::Required(shape.required as u16 + 1));
                shape.required += 1;
            }
        }
        CodeMutation::UpvalueCount(index, count) => {
            chunks[chunk_at(index)]
                .upvalue_names
                .resize(usize::from(*count), Ident::new("fuzz"));
        }
        CodeMutation::SwapConstants(index, a, b) => {
            let constants = &mut chunks[chunk_at(index)].constants;
            if !constants.is_empty() {
                let (a, b) = (a.index(constants.len()), b.index(constants.len()));
                constants.swap(a, b);
            }
        }
        CodeMutation::ToggleGenerator(index) => {
            let chunk = &mut chunks[chunk_at(index)];
            chunk.is_generator = !chunk.is_generator;
        }
    }
    // Keeps source positions valid so the decoder reaches the verifier.
    for chunk in chunks.iter_mut() {
        let len = chunk.code.len();
        chunk.lines.retain(|line| line.pc_start < len);
    }
}

fn run_with_limits(bytes: &[u8]) {
    let mut engine = engine();
    engine.set_timeout(std::time::Duration::from_millis(50));
    engine.set_max_call_stack_depth(64);
    if let Ok(program) = load(&mut engine, bytes) {
        let _ = engine.eval_compiled(&program, markdown("# a\n\n## b\n").into_iter());
    }
}

proptest! {
    #[test]
    fn test_load_mqc_never_panics_on_arbitrary_bytes(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = load(&mut engine(), &bytes);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn test_load_mqc_never_panics_on_mutated_bytecode(
        program in prop::sample::select(FUZZ_PROGRAMS),
        part in any::<Index>(),
        mutations in prop::collection::vec(code_mutation(), 1..4),
    ) {
        let bytes = edit_parts(&compile(program), |parts| {
            let chunks = &mut parts[part.index(parts.len())];
            for mutation in &mutations {
                apply_code_mutation(chunks, mutation);
            }
        });
        run_with_limits(&bytes);
    }

    #[test]
    fn test_load_mqc_never_panics_on_corrupted_code_bytes(
        program in prop::sample::select(FUZZ_PROGRAMS),
        edits in prop::collection::vec((any::<Index>(), any::<u8>()), 1..4),
    ) {
        let bytes = rewrite(&compile(program), |sections| {
            let code = sections.iter_mut().find(|section| section.tag == CODE).unwrap();
            let mut payload = code.payload.to_vec();
            for (index, value) in &edits {
                let position = index.index(payload.len());
                payload[position] = *value;
            }
            code.payload = Cow::Owned(payload);
        });
        run_with_limits(&bytes);
    }
}
