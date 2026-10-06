//! Markdown node types: which kinds a value can have, and how conditions narrow them.

use mq_check::TypeChecker;
use mq_hir::{Hir, SymbolKind};
use rstest::rstest;

/// The display types of every reference to `name`, in source order.
fn ref_types(code: &str, name: &str) -> Vec<String> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    let mut checker = TypeChecker::new();
    let errors = checker.check(&hir);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");

    let mut refs: Vec<_> = hir
        .symbols()
        .filter(|(_, symbol)| symbol.kind == SymbolKind::Ref && symbol.value.as_deref() == Some(name))
        .filter_map(|(id, symbol)| Some((symbol.source.text_range?.start, id)))
        .collect();
    refs.sort();
    refs.into_iter()
        .filter_map(|(_, id)| checker.type_of(id).map(|scheme| scheme.ty.display_renumbered()))
        .collect()
}

/// A markdown value of unknown kind, as the second line of a function body.
const ANY_NODE: &str = "let h = first(to_markdown(\"# a\"))";

#[rstest]
#[case::h1("is_h1", "h1")]
#[case::heading("is_h", "h")]
#[case::code("is_code", "code")]
#[case::list("is_list", "list")]
#[case::text("is_text", "text")]
#[case::mdx(
    "is_mdx",
    "mdx_flow_expression | mdx_jsx_flow_element | mdx_jsx_text_element | mdx_text_expression | mdx_js_esm"
)]
fn test_predicate_narrows_the_branches_of_a_node(#[case] predicate: &str, #[case] kinds: &str) {
    let code = format!("def f():\n  {ANY_NODE}\n  | if ({predicate}(h)):\n    h\n  else:\n    h\nend");
    let types = ref_types(&code, "h");
    assert_eq!(types[1], kinds, "{types:?}");
    assert_eq!(
        types[2],
        format!("markdown - {}", kinds.replace(" | ", " - ")),
        "{types:?}"
    );
}

/// (condition, references to `h` in the condition, then type, else type)
#[rstest]
#[case::or("is_h1(h) || is_h2(h)", 2, "h1 | h2", "markdown - h1 - h2")]
#[case::and("is_h1(h) && is_h(h)", 2, "h1", "markdown")]
#[case::and_in_either_order("is_h(h) && is_h1(h)", 2, "h1", "markdown")]
#[case::not("!is_h1(h)", 1, "markdown - h1", "h1")]
fn test_combined_conditions(
    #[case] condition: &str,
    #[case] condition_refs: usize,
    #[case] then_type: &str,
    #[case] else_type: &str,
) {
    let code = format!("def f():\n  {ANY_NODE}\n  | if ({condition}):\n    h\n  else:\n    h\nend");
    let types = ref_types(&code, "h");
    assert_eq!(types[condition_refs], then_type, "{types:?}");
    assert_eq!(types[condition_refs + 1], else_type, "{types:?}");
}

#[test]
fn test_a_narrowed_node_can_be_narrowed_again() {
    let code = format!("def f():\n  {ANY_NODE}\n  | if (is_h(h)):\n    if (is_h2(h)): h else: h\n  else:\n    h\nend");
    let types = ref_types(&code, "h");
    assert_eq!(types[2], "h2", "{types:?}");
    assert_eq!(types[3], "h1 | h3 | h4 | h5 | h6", "{types:?}");
}

#[test]
fn test_parameter_is_narrowed_to_the_kinds_of_the_predicate() {
    let types = ref_types("def f(x):\n  if (is_code(x)):\n    x\n  else:\n    x\nend", "x");
    assert_eq!(types[1], "code", "{types:?}");
}

#[test]
fn test_structural_selector_condition_narrows_to_the_selected_kinds() {
    let types = ref_types("def f(x):\n  if (.h1):\n    x\n  else:\n    x\nend", "x");
    assert_eq!(types[0], "h1", "{types:?}");
}
