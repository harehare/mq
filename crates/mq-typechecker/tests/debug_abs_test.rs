//! Debug test to understand type checking process for abs(42)

use mq_hir::Hir;
use mq_typechecker::TypeChecker;

#[test]
fn debug_abs_typechecking() {
    let mut hir = Hir::default();
    hir.builtin.disabled = false;
    hir.add_builtin();
    hir.add_code(None, "abs(42)");

    println!("\n===== Starting type checking =====");
    let mut checker = TypeChecker::new();
    let errors = checker.check(&hir);

    println!("\nType checking errors: {:?}", errors);

    for e in &errors {
        println!("\nError details: {:#?}", e);
    }
}
