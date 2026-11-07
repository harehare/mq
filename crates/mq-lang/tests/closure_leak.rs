//! Evaluating a program must not leave allocations behind once the engine is dropped.

use proptest::prelude::*;
use rstest::rstest;
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Mutex;

thread_local! {
    // Per thread, so that tests running in parallel do not count each other's allocations.
    static LIVE: Cell<isize> = const { Cell::new(0) };
}

fn count(delta: isize) {
    let _ = LIVE.try_with(|live| live.set(live.get() + delta));
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(1);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        count(-1);
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Allocations still live after evaluating `code` on a fresh engine that was then dropped.
fn live_after(code: &str) -> isize {
    let before = LIVE.with(Cell::get);
    {
        let mut engine = mq_lang::DefaultEngine::default();
        let _ = engine.eval(code, mq_lang::null_input().into_iter());
    }
    LIVE.with(Cell::get) - before
}

/// With `sync`, the cycle collector's candidates are shared by every thread, so a collection on
/// one test thread can free what another test allocated. Tests therefore take turns.
static SERIAL: Mutex<()> = Mutex::new(());

/// Evaluates `code` twice: the first run fills process-wide caches that stay allocated by design.
fn leaked_by(code: &str) -> isize {
    let _serial = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    live_after(code);
    live_after(code)
}

#[rstest]
#[case::builtin_in("in")]
#[case::builtin_in_call("in([[1, [2]]], [1, [2]])")]
#[case::recursive_def_returning_a_closure("def f(): fn(): f();; | 1")]
#[case::recursive_def_over_a_closure("def f(v): all(v, fn(x): f(x);); | f")]
#[case::recursive_def_returned_from_a_call("def f(n): fn(): f;; | let g = f(1) | g()")]
#[case::recursive_def_with_a_default("def f(n, g = fn(): f;): n; | f(1)")]
#[case::closure_stored_in_its_own_variable("var a = 0 | a = fn(x): a; | 1")]
#[case::closure_stored_through_another_local("var a = 0 | let g = fn(x): a; | a = g | 1")]
#[case::chain_of_closures("var a = 0 | let g = fn(x): a; | let h = fn(x): g(x); | a = h | 1")]
#[case::two_variables_capturing_each_other("var a = 0 | var b = 0 | a = fn(x): b; | b = fn(x): a; | 1")]
#[case::closure_inside_an_array("var a = [] | let g = fn(x): len(a); | a = [g] | 1")]
#[case::closure_inside_a_dict("var a = {} | let g = fn(x): len(a); | a = {\"f\": g} | 1")]
#[case::closure_appended_to_an_array("var a = [] | let g = fn(x): len(a); | a += [g] | 1")]
#[case::cycle_inside_a_function("def mk(): var a = 0 | let g = fn(x): a; | a = g | 1; | mk()")]
#[case::cycle_in_an_array_returned_from_a_function(
    "def mk(): var a = [] | let g = fn(x): len(a); | a = [g] | [g]; | let r = mk() | 1"
)]
#[case::cycle_next_to_a_coroutine(
    "def gen(): yield: 1 | yield: 2; | var c = gen() | let f = fn(x): next(c); | c = [c, f] | 1"
)]
#[case::cycle_repeated_in_a_loop(
    "var i = 0 | while (i < 3000): var a = [] | let g = fn(x): len(a); | a = [g] | i += 1; | 1"
)]
#[case::cycle_made_by_every_call(
    "def mk(): var a = 0 | let g = fn(x): len(a); | a = [g] | 1; | var i = 0 | while (i < 3000): mk() | i += 1; | i"
)]
fn programs_are_freed_with_the_engine(#[case] code: &str) {
    assert_eq!(leaked_by(code), 0, "leaked evaluating {code}");
}

mod generated {
    use super::*;

    /// A statement that rebinds a variable to something holding a closure.
    fn statement() -> impl Strategy<Value = String> {
        let var = prop::sample::select(vec!["a", "b", "c"]);
        let closure = prop::sample::select(vec!["g0", "g1", "g2"]);
        prop_oneof![
            (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} = {g}")),
            (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} = [{g}]")),
            (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} = {{\"k\": {g}}}")),
            (var.clone(), closure.clone()).prop_map(|(x, g)| format!("{x} += [{g}]")),
            (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = fn(v): {y};")),
            (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = [fn(v): {y};]")),
            (var.clone(), var.clone()).prop_map(|(x, y)| format!("{x} = {y}")),
            (closure, var).prop_map(|(g, y)| format!("let {g} = fn(v): {y};")),
        ]
    }

    #[derive(Debug, Clone, Copy)]
    enum Shape {
        TopLevel,
        InFunction,
        InLoop,
    }

    fn program(statements: &[String], shape: Shape) -> String {
        let body = statements.join(" | ");
        let setup = "var a = 0 | var b = [] | var c = {} | \
                     let g0 = fn(v): a; | let g1 = fn(v): b; | let g2 = fn(v): c;";
        match shape {
            Shape::TopLevel => format!("{setup} | {body} | 1"),
            Shape::InFunction => format!("def run(): {setup} | {body} | 1; | run()"),
            Shape::InLoop => {
                format!("def run(): {setup} | {body} | 1; | var i = 0 | while (i < 3): run() | i += 1; | i")
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// Whatever closures end up in whichever variables, nothing outlives the engine,
        /// including when the program stops early on a runtime error.
        #[test]
        fn closures_stored_in_variables_are_freed(
            statements in prop::collection::vec(statement(), 1..8),
            shape in prop_oneof![Just(Shape::TopLevel), Just(Shape::InFunction), Just(Shape::InLoop)],
        ) {
            let code = program(&statements, shape);
            prop_assert_eq!(leaked_by(&code), 0, "leaked evaluating {}", code);
        }
    }
}
