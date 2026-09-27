use smol_str::SmolStr;

use crate::{Hir, ScopeId, SourceId, Symbol, SymbolId, SymbolKind};

impl Hir {
    /// Resolves references that are unresolved or were resolved across sources.
    ///
    /// Scope-chain resolutions depend only on their own source, so they stay valid until
    /// that source is replaced, which drops them from `references`.
    pub fn resolve(&mut self) {
        let symbols_to_resolve: Vec<_> = self
            .symbols
            .iter()
            .filter_map(|(ref_symbol_id, ref_symbol)| match &ref_symbol.kind {
                SymbolKind::Ref
                | SymbolKind::Call
                | SymbolKind::CallDynamic
                | SymbolKind::Argument
                | SymbolKind::QualifiedAccess
                    if !self.references.contains_key(&ref_symbol_id)
                        || self.fallback_references.contains(&ref_symbol_id) =>
                {
                    ref_symbol
                        .value
                        .clone()
                        .map(|name| (ref_symbol_id, ref_symbol.scope, name))
                }
                _ => None,
            })
            .collect();

        let mut include_source_ids = None;

        for (ref_symbol_id, scope, ref_name) in symbols_to_resolve {
            if let Some(symbol_id) = self.resolve_ref_symbol_of_scope(scope, &ref_name, ref_symbol_id) {
                self.references.insert(ref_symbol_id, symbol_id);
                self.fallback_references.remove(&ref_symbol_id);
                continue;
            }

            let source_ids = include_source_ids.get_or_insert_with(|| self.include_source_ids());
            if let Some(symbol_id) = self.resolve_ref_symbol_of_source(source_ids, &ref_name) {
                self.references.insert(ref_symbol_id, symbol_id);
                self.fallback_references.insert(ref_symbol_id);
            } else {
                self.references.remove(&ref_symbol_id);
                self.fallback_references.remove(&ref_symbol_id);
            }
        }
    }

    #[inline(always)]
    pub fn resolve_reference_symbol(&self, ref_symbol_id: SymbolId) -> Option<SymbolId> {
        self.references.get(&ref_symbol_id).copied()
    }

    #[inline(always)]
    fn include_source_ids(&self) -> Vec<SourceId> {
        let mut source_ids = Vec::new();

        for (_, symbol) in &self.symbols {
            match symbol.kind {
                SymbolKind::Include(source_id) | SymbolKind::Import(source_id) | SymbolKind::Module(source_id) => {
                    source_ids.push(source_id);
                }
                _ => {}
            }
        }

        source_ids.push(self.builtin.source_id);

        source_ids
    }

    #[inline(always)]
    fn get_symbol_priority_for_cross_source(&self, symbol_kind: &SymbolKind) -> u8 {
        match symbol_kind {
            SymbolKind::Function(_) => 0,
            SymbolKind::Variable | SymbolKind::DestructuringBinding => 1,
            SymbolKind::Parameter => 2,
            SymbolKind::PatternVariable { .. } => 2,
            SymbolKind::Ident => 2,
            SymbolKind::Argument => 3,
            _ => 4,
        }
    }

    fn resolve_ref_symbol_of_source(&self, source_ids: &[SourceId], ref_name: &SmolStr) -> Option<SymbolId> {
        self.name_index
            .get(ref_name)
            .into_iter()
            .flatten()
            .filter_map(|&symbol_id| {
                let symbol = self.symbols.get(symbol_id)?;
                let source_id = symbol.source.source_id?;
                (source_ids.contains(&source_id) && Self::is_resolvable_target(symbol))
                    .then(|| (self.get_symbol_priority_for_cross_source(&symbol.kind), symbol_id))
            })
            .min_by_key(|(priority, _)| *priority)
            .map(|(_, symbol_id)| symbol_id)
    }

    #[inline(always)]
    fn is_resolvable_target(symbol: &Symbol) -> bool {
        symbol.is_function()
            || symbol.is_parameter()
            || symbol.is_variable()
            || symbol.is_argument()
            || symbol.is_pattern_variable()
            || symbol.is_ident()
    }

    #[inline(always)]
    fn get_symbol_priority_for_scope(&self, symbol_kind: &SymbolKind) -> u8 {
        match symbol_kind {
            SymbolKind::Argument => 0,
            SymbolKind::Parameter => 1,
            SymbolKind::PatternVariable { .. } => 1,
            SymbolKind::Ident => 2,
            SymbolKind::Variable | SymbolKind::DestructuringBinding => 3,
            SymbolKind::Function(_) => 4,
            _ => 5,
        }
    }

    fn resolve_ref_symbol_of_scope(
        &self,
        scope_id: ScopeId,
        ref_name: &SmolStr,
        ref_symbol_id: SymbolId,
    ) -> Option<SymbolId> {
        let ref_start_line = self
            .symbols
            .get(ref_symbol_id)
            .and_then(|s| s.source.text_range)
            .map(|r| r.start.line);
        let candidates = self.name_index.get(ref_name).map(Vec::as_slice).unwrap_or_default();
        let mut scope_id = Some(scope_id);

        while let Some(current_scope_id) = scope_id {
            // Lowest priority wins; among equal priorities, prefer the definition closest
            // to (but before) the ref, i.e. the highest line.
            let best = candidates
                .iter()
                .filter_map(|&symbol_id| {
                    if symbol_id == ref_symbol_id {
                        return None;
                    }
                    let symbol = self.symbols.get(symbol_id)?;
                    if symbol.scope != current_scope_id || !Self::is_resolvable_target(symbol) {
                        return None;
                    }
                    // `let` bindings must be declared before the use site; functions allow forward references.
                    let line = symbol.source.text_range.map(|r| r.start.line);
                    if symbol.is_variable()
                        && let (Some(ref_line), Some(def_line)) = (ref_start_line, line)
                        && def_line > ref_line
                    {
                        return None;
                    }
                    Some((
                        self.get_symbol_priority_for_scope(&symbol.kind),
                        std::cmp::Reverse(line.unwrap_or(0)),
                        symbol_id,
                    ))
                })
                .min_by_key(|(priority, line, _)| (*priority, *line));

            if let Some((_, _, symbol_id)) = best {
                return Some(symbol_id);
            }

            scope_id = self.scopes.get(current_scope_id).and_then(|scope| scope.parent_id);
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;
    use smol_str::SmolStr;
    use url::Url;

    use crate::{Hir, SymbolKind};

    /// (ref url, ref name, ref line, target url, target name, target line), independent of slot ids.
    type Resolution = (String, SmolStr, u32, String, SmolStr, u32);

    fn resolutions(hir: &Hir) -> Vec<Resolution> {
        let describe = |id| {
            let symbol = &hir.symbols[id];
            let url = symbol
                .source
                .source_id
                .and_then(|source_id| hir.url_by_source(&source_id))
                .map(Url::to_string)
                .unwrap_or_default();
            let line = symbol.source.text_range.map_or(0, |r| r.start.line);
            (url, symbol.value.clone().unwrap_or_default(), line)
        };
        let mut pairs: Vec<Resolution> = hir
            .references
            .iter()
            .map(|(ref_id, def_id)| {
                let (ru, rn, rl) = describe(*ref_id);
                let (du, dn, dl) = describe(*def_id);
                (ru, rn, rl, du, dn, dl)
            })
            .collect();
        pairs.sort();
        pairs
    }

    fn hir_without_builtin() -> Hir {
        let mut hir = Hir::default();
        hir.builtin.disabled = true;
        hir
    }

    fn add(hir: &mut Hir, url: &str, code: &str) {
        let (nodes, _) = mq_lang::parse_recovery(code);
        hir.add_nodes(Url::parse(url).unwrap(), &nodes);
    }

    /// Kind of the symbol that the call named `name` in `url` resolves to.
    fn call_target(hir: &Hir, url: &str, name: &str) -> Option<(SymbolKind, String)> {
        let source_id = hir.source_by_url(&Url::parse(url).unwrap())?;
        let (call_id, _) = hir.symbols().find(|(_, s)| {
            s.source.source_id == Some(source_id)
                && s.value.as_deref() == Some(name)
                && matches!(s.kind, SymbolKind::Call | SymbolKind::Ref)
        })?;
        let target = &hir.symbols[hir.resolve_reference_symbol(call_id)?];
        let target_url = hir.url_by_source(&target.source.source_id?)?.to_string();
        Some((target.kind.clone(), target_url))
    }

    const USER: &str = "file:///user.mq";
    const LIB_A: &str = "file:///a.mq";
    const LIB_B: &str = "file:///b.mq";

    #[test]
    fn test_unresolved_ref_resolves_once_a_module_defines_it() {
        let mut hir = hir_without_builtin();
        add(&mut hir, USER, "helper(1)");
        assert_eq!(call_target(&hir, USER, "helper"), None);

        add(&mut hir, LIB_A, "module a: def helper(x): x; end");
        assert_eq!(
            call_target(&hir, USER, "helper").map(|(_, url)| url),
            Some(LIB_A.to_string())
        );
    }

    #[test]
    fn test_ref_becomes_unresolved_when_its_target_is_removed() {
        let mut hir = hir_without_builtin();
        add(&mut hir, LIB_A, "module a: def helper(x): x; end");
        add(&mut hir, USER, "helper(1)");
        assert!(call_target(&hir, USER, "helper").is_some());

        add(&mut hir, LIB_A, "module a: def other(x): x; end");
        assert_eq!(call_target(&hir, USER, "helper"), None);
        assert!(hir.fallback_references.is_empty());
    }

    #[test]
    fn test_cross_source_ref_switches_to_higher_priority_definition() {
        let mut hir = hir_without_builtin();
        add(&mut hir, LIB_A, "module a: let helper = 1 end");
        add(&mut hir, USER, "helper");
        assert!(matches!(
            call_target(&hir, USER, "helper"),
            Some((SymbolKind::Variable, _))
        ));

        // A function outranks a variable across sources, so the existing resolution must be revisited.
        add(&mut hir, LIB_B, "module b: def helper(): 1; end");
        assert_eq!(
            call_target(&hir, USER, "helper"),
            Some((SymbolKind::Function(Vec::new()), LIB_B.to_string()))
        );
    }

    #[test]
    fn test_scope_resolution_is_kept_and_not_tracked_as_fallback() {
        let mut hir = hir_without_builtin();
        add(&mut hir, USER, "def helper(): 1; | helper()");
        add(&mut hir, LIB_A, "module a: def helper(): 2; end");

        assert_eq!(
            call_target(&hir, USER, "helper").map(|(_, url)| url),
            Some(USER.to_string())
        );
        assert!(hir.fallback_references.is_empty());
    }

    #[test]
    fn test_builtin_calls_resolve_to_builtin_after_re_add() {
        let mut hir = Hir::default();
        add(&mut hir, USER, "upcase() | to_string(1)");
        add(&mut hir, USER, "upcase() | to_string(1)");

        for name in ["upcase", "to_string"] {
            let (kind, _) = hir
                .symbols()
                .find(|(_, s)| s.value.as_deref() == Some(name) && matches!(s.kind, SymbolKind::Call))
                .and_then(|(id, _)| hir.resolve_reference_symbol(id))
                .map(|id| (hir.symbols[id].kind.clone(), id))
                .unwrap();
            assert!(matches!(kind, SymbolKind::Function(_)), "{name}: {kind:?}");
        }
    }

    #[rstest]
    #[case::shadowed_let("let x = 1 | let x = 2 | x", 1)]
    #[case::param_over_function("def x(): 1; | def f(x): x;", 1)]
    #[case::forward_function_ref("f() | def f(): 1;", 1)]
    #[case::let_on_a_later_line_is_skipped("x\n| let x = 1", 0)]
    fn test_scope_resolution_matches_fresh_hir_after_re_add(#[case] code: &str, #[case] expected_refs: usize) {
        let mut fresh = hir_without_builtin();
        add(&mut fresh, USER, code);
        let mut reused = hir_without_builtin();
        add(&mut reused, USER, code);
        add(&mut reused, USER, code);

        let expected = resolutions(&fresh);
        assert_eq!(
            expected.iter().filter(|r| r.1 == "x" || r.1 == "f").count(),
            expected_refs
        );
        assert_eq!(resolutions(&reused), expected);
    }

    fn statement() -> impl Strategy<Value = String> {
        let (f, v) = (0..3usize, 0..3usize);
        prop_oneof![
            (f.clone(), v.clone()).prop_map(|(i, j)| format!("def f{i}(x): x + v{j};")),
            (v.clone(), f.clone()).prop_map(|(i, j)| format!("let v{i} = f{j}(1)")),
            v.clone().prop_map(|i| format!("v{i}")),
            (f.clone(), v.clone()).prop_map(|(i, j)| format!("f{i}(v{j})")),
            (v.clone(), v.clone(), f.clone()).prop_map(|(i, j, k)| format!("if (v{i}): v{j} else: f{k}(1)")),
            v.prop_map(|i| format!("fn(x): x + v{i};")),
        ]
    }

    fn program() -> impl Strategy<Value = String> {
        prop::collection::vec(statement(), 1..6).prop_map(|stmts| stmts.join(" | "))
    }

    fn module() -> impl Strategy<Value = String> {
        prop::collection::vec((0..3usize, 0..3usize), 1..4).prop_map(|defs| {
            let body: Vec<String> = defs
                .into_iter()
                .map(|(i, j)| format!("def f{i}(x): x + v{j};"))
                .collect();
            format!("module m: {} end", body.join(" | "))
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_re_adding_source_matches_fresh_hir(code in program()) {
            let mut fresh = hir_without_builtin();
            add(&mut fresh, USER, &code);
            let mut reused = hir_without_builtin();
            add(&mut reused, USER, &code);
            add(&mut reused, USER, &code);

            prop_assert_eq!(resolutions(&reused), resolutions(&fresh));
            prop_assert_eq!(reused.scopes.len(), fresh.scopes.len());
            prop_assert_eq!(reused.symbols.len(), fresh.symbols.len());
        }

        #[test]
        fn prop_incremental_multi_source_matches_fresh_hir(user in program(), lib in module()) {
            let mut fresh = hir_without_builtin();
            add(&mut fresh, LIB_A, &lib);
            add(&mut fresh, USER, &user);

            let mut incremental = hir_without_builtin();
            add(&mut incremental, USER, &user);
            add(&mut incremental, LIB_A, &lib);
            add(&mut incremental, LIB_A, &lib);
            add(&mut incremental, USER, &user);

            prop_assert_eq!(resolutions(&incremental), resolutions(&fresh));
            prop_assert_eq!(incremental.scopes.len(), fresh.scopes.len());
        }
    }
}
