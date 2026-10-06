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

fn errors(code: &str) -> Vec<mq_check::TypeError> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    TypeChecker::new().check(&hir)
}

/// The type of the selector symbol with the given name, e.g. `.depth`.
fn selector_type(code: &str, selector: &str) -> String {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    let mut checker = TypeChecker::new();
    let errors = checker.check(&hir);
    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    let (id, _) = hir
        .symbols()
        .find(|(_, symbol)| matches!(symbol.kind, SymbolKind::Selector(_)) && symbol.value.as_deref() == Some(selector))
        .unwrap_or_else(|| panic!("no selector {selector}"));
    checker.type_of(id).unwrap().ty.display_renumbered()
}

#[rstest]
#[case::heading_depth("to_markdown(\"# a\") | first() | .h1.depth", ".depth", "number")]
#[case::required_attr_of_one_kind("to_markdown(\"x\") | first() | .code.fence", ".fence", "bool")]
#[case::optional_attr_may_be_none("to_markdown(\"x\") | first() | .code.lang", ".lang", "(string | none)")]
#[case::list_checked_is_optional("to_markdown(\"- a\") | first() | .list.checked", ".checked", "(bool | none)")]
#[case::unknown_kind_keeps_the_plain_type("to_markdown(\"x\") | first() | .lang", ".lang", "string")]
fn test_attribute_type_depends_on_the_node_kind(#[case] code: &str, #[case] selector: &str, #[case] expected: &str) {
    assert_eq!(selector_type(code, selector), expected);
}

#[rstest]
#[case::depth_of_code("to_markdown(\"x\") | first() | .code.depth", "depth")]
#[case::lang_of_heading("to_markdown(\"x\") | first() | .h1.lang", "lang")]
fn test_attribute_no_kind_has_is_an_error(#[case] code: &str, #[case] attr: &str) {
    let errors = errors(code);
    assert!(
        matches!(errors.as_slice(), [mq_check::TypeError::UndefinedAttribute { attr: a, .. }] if a == attr),
        "{errors:?}"
    );
}

#[test]
fn test_attribute_of_a_narrowed_variable_is_accepted() {
    // References that carry a selector (`x.depth`) are not narrowed, so only the declared type
    // is checked here.
    let narrowed = "def f(x):\n  if (is_h1(x)):\n    x.depth\n  else:\n    0\nend";
    assert!(errors(narrowed).is_empty());
}

#[rstest]
#[case::number_attribute(r##"let n = first(to_markdown("# a")) | attr(n, "depth") + 1"##)]
#[case::string_attribute(r##"to_markdown("# a") | first() | attr("value") | upcase()"##)]
fn test_attr_call_result_is_decided_by_its_use(#[case] code: &str) {
    assert!(errors(code).is_empty());
}

#[rstest]
#[case::code(r#"to_code("a", "rust")"#, "code")]
#[case::heading(r#"to_h("a", 2)"#, "h")]
#[case::link(r#"to_link("u", "t", "d")"#, "link")]
#[case::strong(r#"to_strong("a")"#, "strong")]
#[case::list(r#"to_md_list("a", 0)"#, "list")]
#[case::horizontal_rule("to_hr()", "Horizontal_rule")]
#[case::setter_keeps_the_kind(r#"set_check(to_md_list("a", 0), true)"#, "list")]
fn test_builtin_constructors_return_their_kind(#[case] expr: &str, #[case] expected: &str) {
    let types = ref_types(&format!("let n = {expr}\n| n"), "n");
    assert_eq!(types, vec![expected.to_string()]);
}

#[test]
fn test_attribute_of_a_constructed_node_is_checked() {
    let errors = errors("let c = to_code(\"a\", \"rust\")\n| c.depth");
    assert!(
        matches!(errors.as_slice(), [mq_check::TypeError::UndefinedAttribute { .. }]),
        "{errors:?}"
    );
}

fn errors_with_input(code: &str, input_type: &str) -> Vec<mq_check::TypeError> {
    let mut hir = Hir::default();
    hir.add_code(None, code);
    TypeChecker::with_options(mq_check::TypeCheckerOptions {
        input_type: Some(mq_check::type_expr::parse_type(input_type).unwrap()),
        ..Default::default()
    })
    .check(&hir)
}

#[rstest]
#[case::heading_has_depth(".depth", "h1 | h2", true)]
#[case::code_has_no_depth(".depth", "code", false)]
#[case::code_has_lang(".lang", "code", true)]
#[case::unknown_markdown_is_accepted(".depth", "markdown", true)]
#[case::kind_selector_on_matching_input(".h1.depth", "h", true)]
#[case::kind_selector_then_missing_attribute(".h1.lang", "h", false)]
fn test_declared_input_type_checks_selectors(#[case] query: &str, #[case] input: &str, #[case] ok: bool) {
    let errors = errors_with_input(query, input);
    assert_eq!(errors.is_empty(), ok, "{errors:?}");
}

#[test]
fn test_without_a_declared_input_type_the_input_is_unknown() {
    assert!(errors(".depth").is_empty());
}
