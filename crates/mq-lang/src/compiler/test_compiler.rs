//! Comprehensive tests for the compiler implementation.

#[cfg(test)]
mod tests {
    use crate::DefaultEngine;
    use crate::eval::runtime_value::RuntimeValue;

    #[test]
    fn test_comprehensive_compiler_tree_walker_equivalence() {
        let test_cases = vec![
            // Literals
            ("42", vec![RuntimeValue::None]),
            ("\"hello\"", vec![RuntimeValue::None]),
            ("true", vec![RuntimeValue::None]),
            ("false", vec![RuntimeValue::None]),
            // Self
            ("self", vec![RuntimeValue::String("test".to_string())]),
            // Nodes
            ("nodes", vec![RuntimeValue::String("test".to_string())]),
            // Arithmetic with builtins
            ("add(\" world\")", vec![RuntimeValue::String("hello".to_string())]),
            ("add(1, 2)", vec![RuntimeValue::None]),
            ("sub(5, 3)", vec![RuntimeValue::None]),
            ("mul(2, 3)", vec![RuntimeValue::None]),
            ("div(10, 2)", vec![RuntimeValue::None]),
            // Control flow
            ("if true: \"yes\" else: \"no\"", vec![RuntimeValue::None]),
            ("if false: \"yes\" else: \"no\"", vec![RuntimeValue::None]),
            ("if true: \"yes\"", vec![RuntimeValue::None]),
            ("if false: \"yes\"", vec![RuntimeValue::None]),
            // And/Or
            ("true and true", vec![RuntimeValue::None]),
            ("true and false", vec![RuntimeValue::None]),
            ("false and true", vec![RuntimeValue::None]),
            ("false or true", vec![RuntimeValue::None]),
            ("true or false", vec![RuntimeValue::None]),
            ("false or false", vec![RuntimeValue::None]),
            // Variables
            ("let x = 42; x", vec![RuntimeValue::None]),
            ("let x = \"hello\"; x", vec![RuntimeValue::None]),
            ("var x = 1; x = 2; x", vec![RuntimeValue::None]),
            // Block
            ("do let x = 42; x end", vec![RuntimeValue::None]),
            // Functions - explicit all args
            ("def foo(x, y): x; foo(5, 10)", vec![RuntimeValue::None]),
            ("fn(x): x; | (42)", vec![RuntimeValue::None]),
            // Loops
            ("foreach (x, [1, 2, 3]): x;", vec![RuntimeValue::None]),
            ("foreach (x, [1, 2, 3]): add(x, 10);", vec![RuntimeValue::None]),
            ("foreach (c, \"abc\"): c;", vec![RuntimeValue::None]),
            // Try-catch
            ("try error(\"fail\") else: \"caught\"", vec![RuntimeValue::None]),
            ("try 42 else: \"error\"", vec![RuntimeValue::None]),
            // Interpolated string
            ("s\"hello ${self}\"", vec![RuntimeValue::String("world".to_string())]),
            ("s\"result: ${add(1, 1)}\"", vec![RuntimeValue::None]),
            // Break/Continue
            (
                "foreach (x, [1, 2, 3]): if x == 2: break else: x;",
                vec![RuntimeValue::None],
            ),
            (
                "foreach (x, [1, 2, 3]): if x == 2: continue else: x;",
                vec![RuntimeValue::None],
            ),
            // Paren
            ("(42)", vec![RuntimeValue::None]),
            ("(add(1, 2))", vec![RuntimeValue::None]),
        ];

        for (code, inputs) in test_cases {
            // Run with tree-walker
            let mut engine_tw = DefaultEngine::default();
            engine_tw.load_builtin_module();
            engine_tw.set_use_compiler(false);
            let result_tw = engine_tw.eval(code, inputs.clone().into_iter());

            // Run with compiler
            let mut engine_comp = DefaultEngine::default();
            engine_comp.load_builtin_module();
            engine_comp.set_use_compiler(true);
            let result_comp = engine_comp.eval(code, inputs.into_iter());

            // Compare results
            assert_eq!(
                result_tw.is_ok(),
                result_comp.is_ok(),
                "Code: {} - Both should succeed or both should fail. TW: {:?}, Comp: {:?}",
                code,
                result_tw.as_ref().err(),
                result_comp.as_ref().err()
            );

            if result_tw.is_ok() {
                let tw_values = result_tw.unwrap_or_default().values().clone();
                let comp_values = result_comp.unwrap().values().clone();
                assert_eq!(tw_values, comp_values, "Code: {} - Results should be identical", code);
            }
        }
    }
}
