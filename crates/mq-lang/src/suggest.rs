//! "Did you mean" suggestions for unresolved builtin/selector names. Namespace-aware:
//! builtins are only compared against builtins, selectors only against selectors.

// Short queries only tolerate a single edit so a 1-2 char typo doesn't fuzzy-match
// an unrelated long name.
fn max_edit_distance(len: usize) -> usize {
    match len {
        0..=3 => 1,
        4..=6 => 2,
        _ => len / 3,
    }
}

// Ties are broken alphabetically so diagnostics stay stable regardless of the
// candidates' iteration order.
#[cold]
fn closest_match<'a, I>(target: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    let allowed = max_edit_distance(target.chars().count());

    candidates
        .into_iter()
        .filter(|candidate| *candidate != target)
        .filter_map(|candidate| {
            let dist = strsim::damerau_levenshtein(target, candidate);
            (dist <= allowed).then_some((dist, candidate))
        })
        .min_by(|(dist_a, cand_a), (dist_b, cand_b)| dist_a.cmp(dist_b).then_with(|| cand_a.cmp(cand_b)))
        .map(|(_, candidate)| candidate)
}

// Excludes purely symbolic aliases (`.**`, `.<>`, `..`) - not meaningful
// edit-distance suggestion targets.
fn is_word_like_selector(selector: &str) -> bool {
    selector
        .strip_prefix('.')
        .and_then(|rest| rest.chars().next())
        .is_some_and(|c| c.is_ascii_alphabetic())
}

/// Closest name to `name` among builtin functions and the given extra candidates (e.g.
/// user-defined functions/variables visible in scope when the lookup failed).
#[cold]
pub(crate) fn suggest_name<'a>(name: &str, extra_candidates: impl IntoIterator<Item = &'a str>) -> Option<String> {
    closest_match(
        name,
        crate::BUILTIN_FUNCTION_DOC
            .keys()
            .map(|s| s.as_str())
            .chain(extra_candidates),
    )
    .map(str::to_string)
}

/// Closest known selector (e.g. `.h1`, `.code`) to `name`, or `None` if nothing is close enough.
#[cold]
pub(crate) fn suggest_selector(name: &str) -> Option<&'static str> {
    closest_match(
        name,
        crate::BUILTIN_SELECTOR_DOC
            .keys()
            .map(|s| s.as_str())
            .filter(|s| is_word_like_selector(s)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case::transposition("slpit", &["split", "join", "map"], Some("split"))]
    #[case::hyphen_vs_underscore("sort-desc", &["sort_desc", "sort_asc"], Some("sort_desc"))]
    #[case::multiple_equally_close_candidates_pick_alphabetical("cat", &["bat", "cot", "car"], Some("bat"))]
    #[case::no_suggestion_when_nothing_close("xyz123", &["split", "join", "map"], None)]
    #[case::exact_match_is_not_suggested("split", &["split", "join"], None)]
    fn test_closest_match(#[case] target: &str, #[case] candidates: &[&str], #[case] expected: Option<&str>) {
        assert_eq!(closest_match(target, candidates.iter().copied()), expected);
    }

    #[test]
    fn test_short_query_does_not_absurdly_match_long_candidate() {
        // A single-character query only tolerates a single edit, so it must not
        // fuzzy-match an unrelated multi-character candidate.
        assert_eq!(closest_match("a", ["array", "abs"].into_iter()), None);
    }

    #[test]
    fn test_suggest_name_finds_real_transposition_typo() {
        assert_eq!(suggest_name("slpit", []), Some("split".to_string()));
    }

    #[test]
    fn test_suggest_name_finds_hyphen_for_underscore_typo() {
        assert_eq!(suggest_name("date-add", []), Some("date_add".to_string()));
    }

    #[test]
    fn test_suggest_name_no_suggestion_for_unrelated_name() {
        assert_eq!(suggest_name("completely_unrelated_xyz", []), None);
    }

    #[test]
    fn test_suggest_name_considers_extra_candidates() {
        assert_eq!(suggest_name("my_fnc", ["my_func"]), Some("my_func".to_string()));
    }

    #[test]
    fn test_suggest_selector_finds_transposition_typo() {
        assert_eq!(suggest_selector(".hedaing"), Some(".heading"));
    }

    #[rstest]
    #[case::word(".heading", true)]
    #[case::word_short(".h", true)]
    #[case::symbolic_angle(".<>", false)]
    #[case::symbolic_stars(".**", false)]
    #[case::symbolic_dots("..", false)]
    #[case::symbolic_brackets(".[]", false)]
    fn test_is_word_like_selector(#[case] selector: &str, #[case] expected: bool) {
        assert_eq!(is_word_like_selector(selector), expected);
    }

    #[test]
    fn test_suggest_selector_no_suggestion_for_unrelated_name() {
        assert_eq!(suggest_selector(".completely_unrelated_xyz"), None);
    }
}
