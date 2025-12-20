use super::ast::node as ast;
use crate::{Ident, Program, Shared, eval::builtin};
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};

type LineCount = usize;

/// Optimization levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OptimizationLevel {
    /// No optimization
    None,
    /// Only function inlining
    InlineOnly,
    /// Full optimization (inlining + constant folding + other optimizations)
    #[default]
    Full,
}

#[derive(Debug)]
pub struct Optimizer {
    constant_table: FxHashMap<Ident, Shared<ast::Expr>>,
    function_table: FxHashMap<Ident, (ast::Params, Program, LineCount)>,
    inline_threshold: LineCount,
    optimization_level: OptimizationLevel,
}

impl Default for Optimizer {
    fn default() -> Self {
        Self {
            constant_table: FxHashMap::with_capacity_and_hasher(200, FxBuildHasher),
            function_table: FxHashMap::with_capacity_and_hasher(100, FxBuildHasher),
            inline_threshold: 3,
            optimization_level: OptimizationLevel::default(),
        }
    }
}

impl Optimizer {
    /// Creates a new optimizer with a custom optimization level
    #[allow(dead_code)]
    pub fn with_level(level: OptimizationLevel) -> Self {
        Self {
            optimization_level: level,
            ..Default::default()
        }
    }

    /// Creates a new optimizer with a custom inline threshold
    #[allow(dead_code)]
    pub fn with_inline_threshold(threshold: usize) -> Self {
        Self {
            inline_threshold: threshold,
            ..Default::default()
        }
    }

    pub fn optimize(&mut self, program: &mut Program) {
        match self.optimization_level {
            OptimizationLevel::None => {
                // No optimization
            }
            OptimizationLevel::InlineOnly => {
                // Only do function inlining
                self.collect_functions_for_inlining(program);
                self.inline_functions(program);
            }
            OptimizationLevel::Full => {
                // Full optimization: inlining + constant folding + dead code elimination
                self.collect_functions_for_inlining(program);

                let used_identifiers = self.collect_used_identifiers(program);

                program.retain_mut(|node| {
                    if let ast::Expr::Let(ident, _) = &*node.expr
                        && !used_identifiers.contains(&ident.name)
                    {
                        self.constant_table.remove(&ident.name);
                        return false;
                    }
                    true
                });

                self.inline_functions(program);

                for node in program {
                    self.optimize_node(node);
                }
            }
        }
    }

    #[inline(always)]
    fn collect_used_identifiers(&mut self, program: &Program) -> FxHashSet<Ident> {
        let mut used_idents = FxHashSet::default();
        for node in program {
            Self::collect_used_identifiers_in_node(node, &mut used_idents);
        }
        used_idents
    }

    fn collect_used_identifiers_in_node(node: &Shared<ast::Node>, used_idents: &mut FxHashSet<Ident>) {
        match &*node.expr {
            ast::Expr::Ident(ident) => {
                used_idents.insert(ident.name);
            }
            ast::Expr::Call(func_ident, args) => {
                used_idents.insert(func_ident.name);
                for arg in args {
                    Self::collect_used_identifiers_in_node(arg, used_idents);
                }
            }
            ast::Expr::CallDynamic(callable, args) => {
                Self::collect_used_identifiers_in_node(callable, used_idents);
                for arg in args {
                    Self::collect_used_identifiers_in_node(arg, used_idents);
                }
            }
            ast::Expr::Let(_, value_node) | ast::Expr::Var(_, value_node) | ast::Expr::Assign(_, value_node) => {
                Self::collect_used_identifiers_in_node(value_node, used_idents);
            }
            ast::Expr::Block(program_nodes) | ast::Expr::Def(_, _, program_nodes) | ast::Expr::Fn(_, program_nodes) => {
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::If(conditions) => {
                for (cond_node_opt, body_node) in conditions {
                    if let Some(cond_node) = cond_node_opt {
                        Self::collect_used_identifiers_in_node(cond_node, used_idents);
                    }
                    Self::collect_used_identifiers_in_node(body_node, used_idents);
                }
            }
            ast::Expr::While(cond_node, program_nodes) => {
                Self::collect_used_identifiers_in_node(cond_node, used_idents);
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::Foreach(_, collection_node, program_nodes) => {
                Self::collect_used_identifiers_in_node(collection_node, used_idents);
                for stmt in program_nodes {
                    Self::collect_used_identifiers_in_node(stmt, used_idents);
                }
            }
            ast::Expr::InterpolatedString(segments) => {
                for segment in segments {
                    if let ast::StringSegment::Expr(node) = segment {
                        Self::collect_used_identifiers_in_node(node, used_idents);
                    }
                }
            }
            ast::Expr::Paren(node) => {
                Self::collect_used_identifiers_in_node(node, used_idents);
            }
            ast::Expr::Try(try_node, catch_node) => {
                Self::collect_used_identifiers_in_node(try_node, used_idents);
                Self::collect_used_identifiers_in_node(catch_node, used_idents);
            }
            ast::Expr::And(expr1, expr2) | ast::Expr::Or(expr1, expr2) => {
                Self::collect_used_identifiers_in_node(expr1, used_idents);
                Self::collect_used_identifiers_in_node(expr2, used_idents);
            }
            ast::Expr::Match(value, arms) => {
                Self::collect_used_identifiers_in_node(value, used_idents);
                for arm in arms {
                    // Collect identifiers from guard
                    if let Some(guard) = &arm.guard {
                        Self::collect_used_identifiers_in_node(guard, used_idents);
                    }
                    // Collect identifiers from body
                    Self::collect_used_identifiers_in_node(&arm.body, used_idents);
                }
            }
            ast::Expr::QualifiedAccess(module_path, access_target) => {
                // Collect all module names in the path
                for module_ident in module_path {
                    used_idents.insert(module_ident.name);
                }
                // Collect from access target
                match access_target {
                    ast::AccessTarget::Call(_, args) => {
                        for arg in args {
                            Self::collect_used_identifiers_in_node(arg, used_idents);
                        }
                    }
                    ast::AccessTarget::Ident(_) => {}
                }
            }
            ast::Expr::Literal(_)
            | ast::Expr::Selector(_)
            | ast::Expr::Nodes
            | ast::Expr::Self_
            | ast::Expr::Include(_)
            | ast::Expr::Import(_)
            | ast::Expr::Module(_, _)
            | ast::Expr::Break
            | ast::Expr::Continue => {}
        }
    }

    /// Collects function definitions that are candidates for inlining
    fn collect_functions_for_inlining(&mut self, program: &Program) {
        let mut exist_function_names: FxHashSet<Ident> = FxHashSet::default();

        for node in program {
            if let ast::Expr::Def(func_ident, params, body) = &*node.expr {
                let line_count = body.len();

                if line_count < self.inline_threshold
                    && !Self::is_used_in_conditionals(func_ident.name, program)
                    && !Self::is_recursive_function(func_ident.name, body)
                    && !Self::is_builtin_functions(func_ident.name)
                {
                    let name = func_ident.name.to_owned();

                    if exist_function_names.contains(&name) {
                        self.function_table.remove(&name);
                    } else {
                        exist_function_names.insert(name);
                        self.function_table
                            .insert(name, (params.clone(), body.clone(), line_count));
                    }
                }
            }
        }
    }

    /// Checks if a function is used within if/elif/else conditions
    #[inline(always)]
    fn is_used_in_conditionals(func_name: Ident, program: &Program) -> bool {
        for node in program {
            if Self::check_conditional_usage_in_node(func_name, node) {
                return true;
            }
        }
        false
    }

    /// Recursively checks if a function is used in conditional contexts within a node
    fn check_conditional_usage_in_node(func_name: Ident, node: &Shared<ast::Node>) -> bool {
        match &*node.expr {
            ast::Expr::If(conditions) => {
                for (cond_node_opt, _) in conditions {
                    if let Some(cond_node) = cond_node_opt
                        && Self::contains_function_call(func_name, cond_node)
                    {
                        return true;
                    }
                }
            }
            ast::Expr::While(cond_node, body) => {
                if Self::contains_function_call(func_name, cond_node) {
                    return true;
                }
                for stmt in body {
                    if Self::check_conditional_usage_in_node(func_name, stmt) {
                        return true;
                    }
                }
            }
            ast::Expr::Def(_, _, body) | ast::Expr::Fn(_, body) => {
                for stmt in body {
                    if Self::check_conditional_usage_in_node(func_name, stmt) {
                        return true;
                    }
                }
            }
            ast::Expr::Foreach(_, collection_node, body) => {
                if Self::contains_function_call(func_name, collection_node) {
                    return true;
                }
                for stmt in body {
                    if Self::check_conditional_usage_in_node(func_name, stmt) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// Checks if a function call exists within a node tree
    fn contains_function_call(func_name: Ident, node: &Shared<ast::Node>) -> bool {
        match &*node.expr {
            ast::Expr::Call(call_ident, args) => {
                if call_ident.name == func_name {
                    return true;
                }
                for arg in args {
                    if Self::contains_function_call(func_name, arg) {
                        return true;
                    }
                }
            }
            ast::Expr::Paren(inner_node) => {
                return Self::contains_function_call(func_name, inner_node);
            }
            ast::Expr::Let(_, value_node) => {
                return Self::contains_function_call(func_name, value_node);
            }
            ast::Expr::Def(ident, params, program) => {
                for param in params {
                    if Self::contains_function_call(func_name, param) {
                        return true;
                    }
                }

                for body_node in program {
                    if Self::contains_function_call(ident.name, body_node) {
                        return true;
                    }
                }
                return false;
            }
            ast::Expr::If(conditions) => {
                for (cond_node_opt, body_node) in conditions {
                    if let Some(cond_node) = cond_node_opt
                        && Self::contains_function_call(func_name, cond_node)
                    {
                        return true;
                    }
                    if Self::contains_function_call(func_name, body_node) {
                        return true;
                    }
                }
            }
            ast::Expr::While(cond_node, body_nodes) => {
                if Self::contains_function_call(func_name, cond_node) {
                    return true;
                }
                for body_node in body_nodes {
                    if Self::contains_function_call(func_name, body_node) {
                        return true;
                    }
                }
            }
            ast::Expr::Foreach(_, collection_node, body_nodes) => {
                if Self::contains_function_call(func_name, collection_node) {
                    return true;
                }
                for body_node in body_nodes {
                    if Self::contains_function_call(func_name, body_node) {
                        return true;
                    }
                }
            }
            ast::Expr::QualifiedAccess(_, access_target) => {
                // Check if qualified access contains function call
                match access_target {
                    ast::AccessTarget::Call(call_ident, args) => {
                        if call_ident.name == func_name {
                            return true;
                        }
                        for arg in args {
                            if Self::contains_function_call(func_name, arg) {
                                return true;
                            }
                        }
                    }
                    ast::AccessTarget::Ident(_) => {}
                }
            }
            _ => {}
        }
        false
    }

    fn is_recursive_function(func_name: Ident, body: &Program) -> bool {
        for node in body {
            if Self::contains_function_call(func_name, node) {
                return true;
            }
        }
        false
    }

    fn is_builtin_functions(func_name: Ident) -> bool {
        builtin::get_builtin_functions(&func_name).is_some()
    }

    /// Applies function inlining to the program
    /// Efficiently applies function inlining to the program.
    #[inline(always)]
    fn inline_functions(&mut self, program: &mut Program) {
        let mut new_program = Vec::with_capacity(program.len());
        for node in program.drain(..) {
            let processed_node = self.inline_functions_in_node(node);
            self.inline_top_level_calls(&mut new_program, processed_node);
        }
        *program = new_program;
    }

    /// Handles inlining of top-level function calls
    fn inline_top_level_calls(&mut self, new_program: &mut Program, node: Shared<ast::Node>) {
        if let ast::Expr::Call(func_ident, args) = &*node.expr
            && let Some((params, body, _)) = self.function_table.get(&func_ident.name)
        {
            // Only inline if the number of arguments matches the number of parameters
            // If params.len() == args.len() + 1, it means implicit first argument is needed,
            // which we cannot handle during static optimization
            if params.len() != args.len() {
                new_program.push(node);
                return;
            }

            let mut param_bindings = FxHashMap::default();
            for (param, arg) in params.iter().zip(args.iter()) {
                if let ast::Expr::Ident(param_ident) = &*param.expr {
                    param_bindings.insert(param_ident.name, arg.clone());
                }
            }

            for body_node in body {
                let inlined_node = Self::substitute_parameters(body_node, &param_bindings);
                new_program.push(inlined_node);
            }

            return;
        }
        new_program.push(node);
    }

    /// Recursively applies function inlining within a node
    fn inline_functions_in_node(&mut self, node: Shared<ast::Node>) -> Shared<ast::Node> {
        let new_expr = match &*node.expr {
            ast::Expr::Def(ident, params, body) => {
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Shared::new(ast::Expr::Def(ident.clone(), params.clone(), new_body))
            }
            ast::Expr::Fn(params, body) => {
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Shared::new(ast::Expr::Fn(params.clone(), new_body))
            }
            ast::Expr::While(cond, body) => {
                let new_cond = self.inline_functions_in_node(cond.clone());
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Shared::new(ast::Expr::While(new_cond, new_body))
            }
            ast::Expr::Foreach(ident, collection, body) => {
                let new_collection = self.inline_functions_in_node(Shared::clone(collection));
                let mut new_body = body.clone();
                self.inline_functions(&mut new_body);
                Shared::new(ast::Expr::Foreach(ident.clone(), new_collection, new_body))
            }
            ast::Expr::If(conditions) => {
                let new_conditions = conditions
                    .iter()
                    .map(|(cond_opt, body)| {
                        let new_cond = cond_opt
                            .as_ref()
                            .map(|cond| self.inline_functions_in_node(Shared::clone(cond)));
                        let new_body = self.inline_functions_in_node(Shared::clone(body));
                        (new_cond, new_body)
                    })
                    .collect();
                Shared::new(ast::Expr::If(new_conditions))
            }
            ast::Expr::Call(func_ident, args) => {
                let new_args: ast::Args = args
                    .iter()
                    .map(|arg| self.inline_functions_in_node(Shared::clone(arg)))
                    .collect();

                // Check if this function call can be inlined
                if let Some((params, body, _)) = self.function_table.get(&func_ident.name) {
                    // Only inline if the number of arguments matches the number of parameters
                    // If params.len() == args.len() + 1, it means implicit first argument is needed,
                    // which we cannot handle during static optimization
                    if params.len() == new_args.len() {
                        // Create parameter bindings
                        let mut param_bindings = FxHashMap::default();
                        for (param, arg) in params.iter().zip(new_args.iter()) {
                            if let ast::Expr::Ident(param_ident) = &*param.expr {
                                param_bindings.insert(param_ident.name, arg.clone());
                            }
                        }
                        // For single-expression functions, return the substituted expression directly
                        if body.len() == 1 {
                            return Self::substitute_parameters(&body[0], &param_bindings);
                        }
                        // For multi-expression functions, we need to create a compound expression
                        // This is a limitation - we can only inline single-expression functions in nested contexts
                        // Multi-expression functions can only be inlined at the top level
                    }
                }

                Shared::new(ast::Expr::Call(func_ident.clone(), new_args))
            }
            ast::Expr::Let(ident, value) => {
                let new_value = self.inline_functions_in_node(Shared::clone(value));
                Shared::new(ast::Expr::Let(ident.clone(), new_value))
            }
            ast::Expr::Paren(inner) => {
                let new_inner = self.inline_functions_in_node(Shared::clone(inner));
                Shared::new(ast::Expr::Paren(new_inner))
            }
            ast::Expr::QualifiedAccess(module_path, access_target) => {
                // Inline functions in qualified access arguments
                let new_access_target = match access_target {
                    ast::AccessTarget::Call(func_name, args) => {
                        let new_args: ast::Args = args
                            .iter()
                            .map(|arg| self.inline_functions_in_node(Shared::clone(arg)))
                            .collect();
                        ast::AccessTarget::Call(func_name.clone(), new_args)
                    }
                    ast::AccessTarget::Ident(_) => access_target.clone(),
                };
                Shared::new(ast::Expr::QualifiedAccess(module_path.clone(), new_access_target))
            }
            _ => Shared::clone(&node.expr),
        };

        Shared::new(ast::Node {
            token_id: node.token_id,
            expr: new_expr,
        })
    }

    fn substitute_parameters(
        node: &Shared<ast::Node>,
        param_bindings: &FxHashMap<Ident, Shared<ast::Node>>,
    ) -> Shared<ast::Node> {
        let new_expr = match &*node.expr {
            ast::Expr::Ident(ident) => {
                if let Some(arg_node) = param_bindings.get(&ident.name) {
                    return arg_node.clone();
                }
                node.expr.clone()
            }
            ast::Expr::Call(func_ident, args) => {
                let substituted_args = args
                    .iter()
                    .map(|arg| Self::substitute_parameters(arg, param_bindings))
                    .collect();
                Shared::new(ast::Expr::Call(func_ident.clone(), substituted_args))
            }
            ast::Expr::Let(ident, value) => {
                let substituted_value = Self::substitute_parameters(value, param_bindings);
                Shared::new(ast::Expr::Let(ident.clone(), substituted_value))
            }
            ast::Expr::Var(ident, value) => {
                let substituted_value = Self::substitute_parameters(value, param_bindings);
                Shared::new(ast::Expr::Var(ident.clone(), substituted_value))
            }
            ast::Expr::Assign(ident, value) => {
                let substituted_value = Self::substitute_parameters(value, param_bindings);
                Shared::new(ast::Expr::Assign(ident.clone(), substituted_value))
            }
            ast::Expr::CallDynamic(callable, args) => {
                let substituted_callable = Self::substitute_parameters(callable, param_bindings);
                let substituted_args = args
                    .iter()
                    .map(|arg| Self::substitute_parameters(arg, param_bindings))
                    .collect();
                Shared::new(ast::Expr::CallDynamic(substituted_callable, substituted_args))
            }
            ast::Expr::And(left, right) => {
                let substituted_left = Self::substitute_parameters(left, param_bindings);
                let substituted_right = Self::substitute_parameters(right, param_bindings);
                Shared::new(ast::Expr::And(substituted_left, substituted_right))
            }
            ast::Expr::Or(left, right) => {
                let substituted_left = Self::substitute_parameters(left, param_bindings);
                let substituted_right = Self::substitute_parameters(right, param_bindings);
                Shared::new(ast::Expr::Or(substituted_left, substituted_right))
            }
            ast::Expr::InterpolatedString(segments) => {
                let substituted_segments = segments
                    .iter()
                    .map(|segment| match segment {
                        ast::StringSegment::Expr(node) => {
                            ast::StringSegment::Expr(Self::substitute_parameters(node, param_bindings))
                        }
                        ast::StringSegment::Text(s) => ast::StringSegment::Text(s.clone()),
                        ast::StringSegment::Env(s) => ast::StringSegment::Env(s.clone()),
                        ast::StringSegment::Self_ => ast::StringSegment::Self_,
                    })
                    .collect();
                Shared::new(ast::Expr::InterpolatedString(substituted_segments))
            }
            ast::Expr::Match(target, arms) => {
                let substituted_target = Self::substitute_parameters(target, param_bindings);
                let substituted_arms = arms
                    .iter()
                    .map(|arm| {
                        let substituted_guard = arm
                            .guard
                            .as_ref()
                            .map(|guard| Self::substitute_parameters(guard, param_bindings));
                        let substituted_body = Self::substitute_parameters(&arm.body, param_bindings);
                        ast::MatchArm {
                            pattern: arm.pattern.clone(),
                            guard: substituted_guard,
                            body: substituted_body,
                        }
                    })
                    .collect();
                Shared::new(ast::Expr::Match(substituted_target, substituted_arms))
            }
            ast::Expr::Module(ident, program) => {
                let substituted_program = program
                    .iter()
                    .map(|node| Self::substitute_parameters(node, param_bindings))
                    .collect();
                Shared::new(ast::Expr::Module(ident.clone(), substituted_program))
            }
            ast::Expr::Try(try_node, catch_node) => {
                let substituted_try = Self::substitute_parameters(try_node, param_bindings);
                let substituted_catch = Self::substitute_parameters(catch_node, param_bindings);
                Shared::new(ast::Expr::Try(substituted_try, substituted_catch))
            }
            ast::Expr::Paren(inner) => {
                let substituted_inner = Self::substitute_parameters(inner, param_bindings);
                Shared::new(ast::Expr::Paren(substituted_inner))
            }
            ast::Expr::QualifiedAccess(module_path, access_target) => {
                // Substitute parameters in qualified access arguments
                let new_access_target = match access_target {
                    ast::AccessTarget::Call(func_name, args) => {
                        let substituted_args = args
                            .iter()
                            .map(|arg| Self::substitute_parameters(arg, param_bindings))
                            .collect();
                        ast::AccessTarget::Call(func_name.clone(), substituted_args)
                    }
                    ast::AccessTarget::Ident(_) => access_target.clone(),
                };
                Shared::new(ast::Expr::QualifiedAccess(module_path.clone(), new_access_target))
            }
            ast::Expr::Block(program_nodes) => {
                let substituted_program = program_nodes
                    .iter()
                    .map(|node| Self::substitute_parameters(node, param_bindings))
                    .collect();
                Shared::new(ast::Expr::Block(substituted_program))
            }
            ast::Expr::If(conditions) => {
                let substituted_conditions = conditions
                    .iter()
                    .map(|(cond_opt, body)| {
                        let substituted_cond = cond_opt
                            .as_ref()
                            .map(|cond| Self::substitute_parameters(cond, param_bindings));
                        let substituted_body = Self::substitute_parameters(body, param_bindings);
                        (substituted_cond, substituted_body)
                    })
                    .collect();
                Shared::new(ast::Expr::If(substituted_conditions))
            }
            ast::Expr::While(cond, body) => {
                let substituted_cond = Self::substitute_parameters(cond, param_bindings);
                let substituted_body = body
                    .iter()
                    .map(|node| Self::substitute_parameters(node, param_bindings))
                    .collect();
                Shared::new(ast::Expr::While(substituted_cond, substituted_body))
            }
            ast::Expr::Foreach(ident, collection, body) => {
                let substituted_collection = Self::substitute_parameters(collection, param_bindings);
                let substituted_body = body
                    .iter()
                    .map(|node| Self::substitute_parameters(node, param_bindings))
                    .collect();
                Shared::new(ast::Expr::Foreach(
                    ident.clone(),
                    substituted_collection,
                    substituted_body,
                ))
            }
            ast::Expr::Def(ident, params, body) => {
                let substituted_body = body
                    .iter()
                    .map(|node| Self::substitute_parameters(node, param_bindings))
                    .collect();
                Shared::new(ast::Expr::Def(ident.clone(), params.clone(), substituted_body))
            }
            ast::Expr::Fn(params, body) => {
                let substituted_body = body
                    .iter()
                    .map(|node| Self::substitute_parameters(node, param_bindings))
                    .collect();
                Shared::new(ast::Expr::Fn(params.clone(), substituted_body))
            }
            _ => node.expr.clone(),
        };

        Shared::new(ast::Node {
            token_id: node.token_id,
            expr: new_expr,
        })
    }

    fn optimize_node(&mut self, node: &mut Shared<ast::Node>) {
        let mut_node = Shared::make_mut(node);
        let mut_expr = Shared::make_mut(&mut mut_node.expr);

        match mut_expr {
            ast::Expr::Call(ident, args) => {
                for arg in args.iter_mut() {
                    self.optimize_node(arg);
                }

                let new_expr = ident.name.resolve_with(|name_str| match (name_str, args.as_slice()) {
                    ("add", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a + *b)))
                        }
                        (ast::Expr::Literal(ast::Literal::String(a)), ast::Expr::Literal(ast::Literal::String(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::String(format!("{}{}", a, b))))
                        }
                        _ => None,
                    },
                    ("sub", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a - *b)))
                        }
                        _ => None,
                    },
                    ("div", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a / *b)))
                        }
                        _ => None,
                    },
                    ("mul", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a * *b)))
                        }
                        _ => None,
                    },
                    ("mod", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::Number(a)), ast::Expr::Literal(ast::Literal::Number(b))) => {
                            Some(ast::Expr::Literal(ast::Literal::Number(*a % *b)))
                        }
                        _ => None,
                    },
                    ("repeat", [arg1, arg2]) => match (&*arg1.expr, &*arg2.expr) {
                        (ast::Expr::Literal(ast::Literal::String(s)), ast::Expr::Literal(ast::Literal::Number(n))) => {
                            Some(ast::Expr::Literal(ast::Literal::String(s.repeat(n.value() as usize))))
                        }
                        _ => None,
                    },
                    ("reverse", [arg1]) => match &*arg1.expr {
                        ast::Expr::Literal(ast::Literal::String(s)) => Some(ast::Expr::Literal(ast::Literal::String(
                            s.chars().rev().collect::<String>(),
                        ))),
                        _ => None,
                    },
                    _ => None,
                });
                if let Some(expr) = new_expr {
                    mut_node.expr = Shared::new(expr);
                }
            }
            ast::Expr::Ident(ident) => {
                if let Some(expr) = self.constant_table.get(&ident.name) {
                    mut_node.expr = Shared::clone(expr);
                }
            }
            ast::Expr::Foreach(_, each_values, program) => {
                self.optimize_node(each_values);
                for node in program {
                    self.optimize_node(node);
                }
            }
            ast::Expr::If(conditions) => {
                for (cond, expr) in conditions.iter_mut() {
                    if let Some(c) = cond {
                        self.optimize_node(c);
                    }
                    self.optimize_node(expr);
                }
            }
            ast::Expr::Let(ident, value) => {
                self.optimize_node(value);
                if let ast::Expr::Literal(_) = &*value.expr {
                    self.constant_table
                        .insert(ident.name.to_owned(), Shared::clone(&value.expr));
                }
            }
            ast::Expr::Def(_, _, program) | ast::Expr::Fn(_, program) => {
                // Save current constant table to prevent leaking function-local constants
                let saved_constant_table = std::mem::take(&mut self.constant_table);

                for node in program {
                    self.optimize_node(node);
                }

                // Restore the outer scope's constant table
                self.constant_table = saved_constant_table;
            }
            ast::Expr::Paren(expr) => {
                self.optimize_node(expr);
            }
            ast::Expr::InterpolatedString(segments) => {
                for segment in segments.iter_mut() {
                    if let ast::StringSegment::Expr(node) = segment {
                        self.optimize_node(node);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::node::{Expr as AstExpr, IdentWithToken, Literal, Node}; // Added Ident
    use rstest::rstest;
    use smallvec::smallvec;

    #[rstest]
    #[case::constant_folding_add(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("add"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                })
            ])]
    #[case::constant_folding_add(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("add"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::String("hello".to_string()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::String("world".to_string()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::String("helloworld".to_string()))),
                })
            ])]
    #[case::constant_folding_sub(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("sub"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_folding_mul(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("mul"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(6.0.into()))),
                })
            ])]
    #[case::constant_folding_div(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("div"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(6.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_folding_mod(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Call(
                        IdentWithToken::new("mod"),
                        smallvec![
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                            }),
                            Shared::new(ast::Node {
                                token_id: 0.into(),
                                expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(3.0.into()))),
                            }),
                        ],
                    )),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(2.0.into()))),
                })
            ])]
    #[case::constant_propagation(
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Let(
                        IdentWithToken::new("x"),
                        Shared::new(ast::Node {
                            token_id: 0.into(),
                            expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                        }),
                    )),
                }),
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Ident(IdentWithToken::new("x"))),
                })
            ],
            vec![
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Let(
                        IdentWithToken::new("x"),
                        Shared::new(ast::Node {
                            token_id: 0.into(),
                            expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                        }),
                    )),
                }),
                Shared::new(ast::Node {
                    token_id: 0.into(),
                    expr: Shared::new(ast::Expr::Literal(ast::Literal::Number(5.0.into()))),
                })
            ])]
    #[case::dead_code_elimination_simple_unused(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("unused_var"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("used_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(20.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("used_var"))),
            }),
        ],
        // Expected: unused_var is removed
        vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("used_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(20.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(20.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_used_variable_kept(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("x"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("x"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_multiple_unused(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("a"), Shared::new(Node { token_id: 1.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("b"), Shared::new(Node { token_id: 3.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("c"), Shared::new(Node { token_id: 5.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(30.0.into()))) }))),
            }),
             Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("c"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("c"), Shared::new(Node { token_id: 5.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(30.0.into()))) }))),
            }),
             Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(30.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_mixed_used_unused(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("unused1"), Shared::new(Node { token_id: 1.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("used1"), Shared::new(Node { token_id: 3.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("unused2"), Shared::new(Node { token_id: 5.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("used1"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(IdentWithToken::new("used1"), Shared::new(Node { token_id: 3.into(), expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))) }))),
            }),
            Shared::new(Node {
                token_id: 6.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(10.0.into()))),
            }),
        ]
    )]
    #[case::dead_code_elimination_unused_constant_candidate(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("const_unused"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(100.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("another_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(200.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("another_var"))),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("another_var"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(200.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(200.0.into()))),
            }),
        ]
    )]
    #[case::constant_folding_repeat(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("repeat"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::String("ab".to_string()))),
                        }),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        }),
                    ],
                )),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::String("ababab".to_string()))),
            }),
        ]
    )]
    #[case::constant_folding_reverse(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("reverse"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 1.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::String("abc".to_string()))),
                        }),
                    ],
                )),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::String("cba".to_string()))),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_expr(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("name"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Hello, ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("name"))),
                    })),
                    ast::StringSegment::Text("!".to_string()),
                ])),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("name"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Hello, ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Alice".to_string()))),
                    })),
                    ast::StringSegment::Text("!".to_string()),
                ])),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_multiple_exprs(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("first"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("last"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Name: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 5.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("first"))),
                    })),
                    ast::StringSegment::Text(" ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 6.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("last"))),
                    })),
                ])),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("first"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("last"),
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 4.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Name: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 5.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Bob".to_string()))),
                    })),
                    ast::StringSegment::Text(" ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 6.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::String("Smith".to_string()))),
                    })),
                ])),
            }),
        ]
    )]
    #[case::constant_folding_interpolated_string_expr_non_const(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("dynamic"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Call(
                            IdentWithToken::new("some_func"),
                            smallvec![],
                        )),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Value: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("dynamic"))),
                    })),
                ])),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("dynamic"),
                    Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Call(
                            IdentWithToken::new("some_func"),
                            smallvec![],
                        )),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 2.into(),
                expr: Shared::new(AstExpr::InterpolatedString(vec![
                    ast::StringSegment::Text("Value: ".to_string()),
                    ast::StringSegment::Expr(Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("dynamic"))),
                    })),
                ])),
            }),
        ]
    )]
    #[case::function_inlining_simple(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("add_one"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("add"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                    }),
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("add_one"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        })
                    ],
                )),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("add_one"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("add"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                    }),
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            // Inlined function call
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(6.0.into()))),
            }),
        ]
    )]
    #[case::function_inlining_not_recursive(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("square"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("mul"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                                    }),
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("square"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        })
                    ],
                )),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("square"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("mul"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                                    }),
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            // Inlined and optimized function call: 3 * 3 = 9
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(9.0.into()))),
            }),
        ]
    )]
    #[case::function_not_inlined_recursive(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("factorial"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("factorial"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Call(
                                            IdentWithToken::new("sub"),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                                                }),
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                                }),
                                            ],
                                        )),
                                    })
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("factorial"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        })
                    ],
                )),
            }),
        ],
        // Should not be inlined because it's recursive
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("factorial"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("factorial"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Call(
                                            IdentWithToken::new("sub"),
                                            smallvec![
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("n"))),
                                                }),
                                                Shared::new(Node {
                                                    token_id: 0.into(),
                                                    expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                                }),
                                            ],
                                        )),
                                    })
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("factorial"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        })
                    ],
                )),
            }),
        ]
    )]
    #[case::function_inlining_multi_line(
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("multi_step"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Let(
                                IdentWithToken::new("temp"),
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Call(
                                        IdentWithToken::new("add"),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 0.into(),
                                                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                            }),
                                            Shared::new(Node {
                                                token_id: 0.into(),
                                                expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                            }),
                                        ],
                                    )),
                                }),
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("mul"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("temp"))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                    }),
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("multi_step"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                        })
                    ],
                )),
            }),
        ],
        vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("multi_step"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                        })
                    ],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Let(
                                IdentWithToken::new("temp"),
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Call(
                                        IdentWithToken::new("add"),
                                        smallvec![
                                            Shared::new(Node {
                                                token_id: 0.into(),
                                                expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                            }),
                                            Shared::new(Node {
                                                token_id: 0.into(),
                                                expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                                            }),
                                        ],
                                    )),
                                }),
                            )),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("mul"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("temp"))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                    }),
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            // Multi-line function inlined - first statement
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("temp"),
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(6.0.into()))), // add(5, 1) = 6
                    }),
                )),
            }),
            // Multi-line function inlined - second statement
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(12.0.into()))), // mul(6, 2) = 12
            }),
        ]
    )]
    fn test(#[case] input: Program, #[case] expected: Program) {
        let mut optimizer = Optimizer::default();
        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);
        assert_eq!(optimized_program, expected);

        // Additionally, for the unused constant candidate test, check constant_table
        if input.len() == 3 && expected.len() == 2 {
            // Heuristic for this specific test case
            if let AstExpr::Let(ident, _) = &*input[0].expr
                && ident.name.as_str() == "const_unused"
            {
                assert!(
                    !optimizer.constant_table.contains_key(&ident.name),
                    "const_unused should be removed from constant_table"
                );
            }
        }
    }

    #[test]
    fn test_inlining_with_custom_threshold() {
        let mut optimizer = Optimizer::with_inline_threshold(1);

        let input = vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("long_func"),
                    smallvec![],
                    vec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                        }),
                    ],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(IdentWithToken::new("long_func"), smallvec![])),
            }),
        ];

        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);

        // Function should not be inlined because it exceeds the threshold
        assert_eq!(optimized_program, input);
    }

    #[test]
    fn test_optimization_level_none() {
        let mut optimizer = Optimizer::with_level(OptimizationLevel::None);

        let input = vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Let(
                    IdentWithToken::new("x"),
                    Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(5.0.into()))),
                    }),
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("add"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        }),
                    ],
                )),
            }),
        ];

        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);

        // No optimization should be applied
        assert_eq!(optimized_program, input);
    }

    #[test]
    fn test_optimization_level_inline_only() {
        let mut optimizer = Optimizer::with_level(OptimizationLevel::InlineOnly);

        let input = vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("double"),
                    smallvec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                    })],
                    vec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Call(
                            IdentWithToken::new("mul"),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                }),
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                }),
                            ],
                        )),
                    })],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("double"),
                    smallvec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                    })],
                )),
            }),
            // This should not be constant-folded in InlineOnly mode
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("add"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                    ],
                )),
            }),
        ];

        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);

        // Function should be inlined, but constant folding should not happen
        let expected = vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("double"),
                    smallvec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                    })],
                    vec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Call(
                            IdentWithToken::new("mul"),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                }),
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                }),
                            ],
                        )),
                    })],
                )),
            }),
            // Inlined function body
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("mul"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                        }),
                    ],
                )),
            }),
            // This add operation should NOT be constant-folded in InlineOnly mode
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("add"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                    ],
                )),
            }),
        ];

        assert_eq!(optimized_program, expected);
    }

    #[test]
    fn test_optimization_level_full() {
        let mut optimizer = Optimizer::with_level(OptimizationLevel::Full);

        let input = vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("double"),
                    smallvec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                    })],
                    vec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Call(
                            IdentWithToken::new("mul"),
                            smallvec![
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                }),
                                Shared::new(Node {
                                    token_id: 0.into(),
                                    expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                }),
                            ],
                        )),
                    })],
                )),
            }),
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("double"),
                    smallvec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Literal(Literal::Number(3.0.into()))),
                    })],
                )),
            }),
            // This should be constant-folded in Full mode
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Call(
                    IdentWithToken::new("add"),
                    smallvec![
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                    ],
                )),
            }),
        ];

        let mut optimized_program = input.clone();
        optimizer.optimize(&mut optimized_program);

        // Both inlining and constant folding should happen
        let expected = vec![
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Def(
                    IdentWithToken::new("double"),
                    smallvec![Shared::new(Node {
                        token_id: 0.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                    })],
                    vec![
                        // The function body remains unchanged, but inlined calls are optimized
                        Shared::new(Node {
                            token_id: 0.into(),
                            expr: Shared::new(AstExpr::Call(
                                IdentWithToken::new("mul"),
                                smallvec![
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                                    }),
                                    Shared::new(Node {
                                        token_id: 0.into(),
                                        expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                                    }),
                                ],
                            )),
                        }),
                    ],
                )),
            }),
            // Inlined and optimized function result: mul(3, 2) = 6
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(6.0.into()))),
            }),
            // Constant-folded add operation
            Shared::new(Node {
                token_id: 0.into(),
                expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
            }),
        ];

        assert_eq!(optimized_program, expected);
    }

    #[test]
    fn test_contains_function_call_in_if_conditions() {
        let func_name = Ident::new("test_func");

        // Test function call in if condition
        let if_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::If(smallvec![(
                Some(Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Call(IdentWithToken::new("test_func"), smallvec![])),
                })),
                Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                })
            )])),
        });

        assert!(Optimizer::contains_function_call(func_name, &if_node));

        // Test function call in if body
        let if_body_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::If(smallvec![(
                Some(Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Bool(true))),
                })),
                Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Call(IdentWithToken::new("test_func"), smallvec![])),
                })
            )])),
        });

        assert!(Optimizer::contains_function_call(func_name, &if_body_node));
    }

    #[test]
    fn test_contains_function_call_in_while_conditions() {
        let func_name = Ident::new("test_func");

        // Test function call in while condition
        let while_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::While(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Call(IdentWithToken::new("test_func"), smallvec![])),
                }),
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                })],
            )),
        });

        assert!(Optimizer::contains_function_call(func_name, &while_node));

        // Test function call in while body
        let while_body_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::While(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Bool(true))),
                }),
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Call(IdentWithToken::new("test_func"), smallvec![])),
                })],
            )),
        });

        assert!(Optimizer::contains_function_call(func_name, &while_body_node));
    }

    #[test]
    fn test_contains_function_call_in_foreach_conditions() {
        let func_name = Ident::new("test_func");

        // Test function call in foreach collection
        let foreach_collection_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::Foreach(
                IdentWithToken::new("item"),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Call(IdentWithToken::new("test_func"), smallvec![])),
                }),
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                })],
            )),
        });

        assert!(Optimizer::contains_function_call(func_name, &foreach_collection_node));

        // Test function call in foreach body
        let foreach_body_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::Foreach(
                IdentWithToken::new("item"),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("items"))),
                }),
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Call(IdentWithToken::new("test_func"), smallvec![])),
                })],
            )),
        });

        assert!(Optimizer::contains_function_call(func_name, &foreach_body_node));
    }

    #[test]
    fn test_contains_function_call_nested_control_structures() {
        let func_name = Ident::new("test_func");

        // Test nested if inside while with function call
        let nested_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::While(
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Bool(true))),
                }),
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::If(smallvec![(
                        Some(Shared::new(Node {
                            token_id: 3.into(),
                            expr: Shared::new(AstExpr::Call(IdentWithToken::new("test_func"), smallvec![],)),
                        })),
                        Shared::new(Node {
                            token_id: 4.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        })
                    )])),
                })],
            )),
        });

        assert!(Optimizer::contains_function_call(func_name, &nested_node));
    }

    #[test]
    fn test_contains_function_call_no_match() {
        let func_name = Ident::new("test_func");
        let different_func = IdentWithToken::new("other_func");

        // Test that it returns false when function name doesn't match
        let if_node = Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::If(smallvec![(
                Some(Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Call(different_func, smallvec![])),
                })),
                Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                })
            )])),
        });

        assert!(!Optimizer::contains_function_call(func_name, &if_node));
    }

    #[rstest]
    #[case::simple(
    vec![
        Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::Let(
                IdentWithToken::new("x"),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                }),
            )),
        }),
        Shared::new(Node {
            token_id: 2.into(),
            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
        }),
        Shared::new(Node {
            token_id: 3.into(),
            expr: Shared::new(AstExpr::Ident(IdentWithToken::new("y"))),
        }),
    ],
    vec!["x", "y"],
    vec!["z"]
)]
    #[case::call_and_let(
    vec![
        Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::Let(
                IdentWithToken::new("a"),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Literal(Literal::Number(2.0.into()))),
                }),
            )),
        }),
        Shared::new(Node {
            token_id: 2.into(),
            expr: Shared::new(AstExpr::Call(
                IdentWithToken::new("foo"),
                smallvec![
                    Shared::new(Node {
                        token_id: 3.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("a"))),
                    }),
                    Shared::new(Node {
                        token_id: 4.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("b"))),
                    }),
                ],
            )),
        }),
    ],
    vec!["foo", "a", "b"],
    vec![]
)]
    #[case::if_and_while(
    vec![
        Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::If(smallvec![
                (
                    Some(Shared::new(Node {
                        token_id: 1.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("cond_var"))),
                    })),
                    Shared::new(Node {
                        token_id: 2.into(),
                        expr: Shared::new(AstExpr::Ident(IdentWithToken::new("body_var"))),
                    }),
                ),
            ])),
        }),
        Shared::new(Node {
            token_id: 3.into(),
            expr: Shared::new(AstExpr::While(
                Shared::new(Node {
                    token_id: 4.into(),
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("while_cond"))),
                }),
                vec![Shared::new(Node {
                    token_id: 5.into(),
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("while_body"))),
                })],
            )),
        }),
    ],
    vec!["cond_var", "body_var", "while_cond", "while_body"],
    vec![]
)]
    #[case::foreach_and_interpolated_string(
    vec![
        Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::Foreach(
                IdentWithToken::new("item"),
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("collection"))),
                }),
                vec![Shared::new(Node {
                    token_id: 2.into(),
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("item"))),
                })],
            )),
        }),
        Shared::new(Node {
            token_id: 3.into(),
            expr: Shared::new(AstExpr::InterpolatedString(vec![
                ast::StringSegment::Text("Hello ".to_string()),
                ast::StringSegment::Expr(Shared::new(Node {
                    token_id: 4.into(),
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("name"))),
                })),
            ])),
        }),
    ],
    vec!["collection", "item", "name"],
    vec![]
)]
    #[case::nested_blocks(
    vec![
        Shared::new(Node {
            token_id: 0.into(),
            expr: Shared::new(AstExpr::Block(vec![
                Shared::new(Node {
                    token_id: 1.into(),
                    expr: Shared::new(AstExpr::Let(
                        IdentWithToken::new("x"),
                        Shared::new(Node {
                            token_id: 2.into(),
                            expr: Shared::new(AstExpr::Literal(Literal::Number(1.0.into()))),
                        }),
                    )),
                }),
                Shared::new(Node {
                    token_id: 3.into(),
                    expr: Shared::new(AstExpr::Ident(IdentWithToken::new("x"))),
                }),
            ])),
        }),
    ],
    vec!["x"],
    vec![]
)]
    fn test_collect_used_identifiers_param(
        #[case] program: Program,
        #[case] expected_present: Vec<&str>,
        #[case] expected_absent: Vec<&str>,
    ) {
        let mut optimizer = Optimizer::default();
        let used = optimizer.collect_used_identifiers(&program);
        for ident in expected_present {
            assert!(
                used.contains(&Ident::new(ident)),
                "Expected identifier '{}' to be present",
                ident
            );
        }
        for ident in expected_absent {
            assert!(
                !used.contains(&Ident::new(ident)),
                "Expected identifier '{}' to be absent",
                ident
            );
        }
    }

    /// Tests for function inlining with parameter substitution
    mod inline_tests {
        use super::*;
        use crate::RuntimeValue;
        use rstest::rstest;

        #[rstest]
        #[case::block_with_param("def c(cc): do cc + 3;; | c(1)", RuntimeValue::Number(4.into()))]
        #[case::let_and_block_with_param("def c(cc): let a = 10 | do cc + a;; | c(1)", RuntimeValue::Number(11.into()))]
        #[case::if_with_param("def c(x): if (x > 0): x + 10 else: x - 10; | c(5)", RuntimeValue::Number(15.into()))]
        #[case::if_elif_with_param("def c(x): if (eq(x, 0)): 0 elif (eq(x, 1)): x + 1 else: x + 2; | c(1)", RuntimeValue::Number(2.into()))]
        #[case::logical_ops_with_param("def c(x, y): and(x > 0, y > 0); | c(5, 3)", RuntimeValue::TRUE)]
        #[case::call_dynamic_with_param("def c(x): let f = fn(a): a + 10; | f(x); | c(5)", RuntimeValue::Number(15.into()))]
        #[case::nested_def_with_param("def outer(x): def inner(y): x + y; | inner(3); | outer(10)", RuntimeValue::Number(13.into()))]
        #[case::interpolated_string_with_param(r#"def c(name): s"Hello, ${name}!"; | c("World")"#, RuntimeValue::String("Hello, World!".to_string()))]
        #[case::try_catch_with_param(r#"def c(x): try: x / 0 catch: "error"; | c(10)"#, RuntimeValue::String("error".to_string()))]
        #[case::multiple_params("def c(a, b, c): do a + b + c;; | c(1, 2, 3)", RuntimeValue::Number(6.into()))]
        #[case::var_with_param("def c(x): var result = x | let result = result + 1 | result; | c(5)", RuntimeValue::Number(6.into()))]
        #[case::and_operator_with_param("def c(x): and(x, true); | c(true)", RuntimeValue::TRUE)]
        #[case::or_operator_with_param("def c(x): or(x, false); | c(true)", RuntimeValue::TRUE)]
        fn test_inline_with_param_substitution(#[case] program_str: &str, #[case] expected: RuntimeValue) {
            let mut engine = crate::DefaultEngine::default();
            engine.load_builtin_module();
            engine.set_optimization_level(OptimizationLevel::Full);
            let result = engine.eval(program_str, crate::null_input().into_iter());
            assert_eq!(result.unwrap()[0], expected);
        }
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use crate::{Arena, DefaultEngine, SharedCell, strategies::*};
    use proptest::prelude::*;
    proptest! {
        #[test]
        fn test_optimization_idempotent(
            (expr_str, _expected) in arb_arithmetic_expr()
        ) {
            let token_arena = Shared::new(SharedCell::new(Arena::new(100)));

            let program = crate::parse(&expr_str, Shared::clone(&token_arena));
            prop_assume!(program.is_ok());

            let mut program = program.unwrap();

            let mut optimizer1 = Optimizer::default();
            optimizer1.optimize(&mut program);
            let first_optimized = program.clone();

            let mut optimizer2 = Optimizer::default();
            optimizer2.optimize(&mut program);
            let second_optimized = program;

            prop_assert_eq!(first_optimized, second_optimized, "Optimization should be idempotent");
        }

        /// Property: Optimization preserves semantics for constant folding
        /// The optimized and non-optimized versions should evaluate to the same value
        #[test]
        fn test_optimization_preserves_semantics_constant_folding(
            (expr_str, expected) in arb_arithmetic_expr()
        ) {
            let token_arena = Shared::new(SharedCell::new(Arena::new(100)));

            let program = crate::parse(&expr_str, Shared::clone(&token_arena));
            prop_assume!(program.is_ok());

            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);

            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok(), "Non-optimized evaluation should succeed");
            prop_assert!(result_opt.is_ok(), "Optimized evaluation should succeed");

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(
                val_no_opt,
                val_opt,
                "Expected value: {}", expected
            );
        }

        /// Property: Nested expressions are also optimized correctly
        #[test]
        fn test_optimization_preserves_semantics_nested(
            expr_str in arb_nested_arithmetic_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);

            let mut engine_opt = DefaultEngine::default();
            let result_opt = engine_opt.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok(), "Non-optimized evaluation should succeed");
            prop_assert!(result_opt.is_ok(), "Optimized evaluation should succeed");

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: InlineOnly optimization level should not affect semantics
        #[test]
        fn test_inline_only_preserves_semantics(
            (expr_str, _expected) in arb_arithmetic_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_inline = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::InlineOnly);

            prop_assert!(result_no_opt.is_ok(), "Non-optimized evaluation should succeed");
            prop_assert!(result_inline.is_ok(), "Inline-only evaluation should succeed");

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_inline = &result_inline.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_inline);
        }

        /// Property: String concatenation optimization preserves semantics
        #[test]
        fn test_optimization_string_concat(
            expr_str in arb_string_concat_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Comparison expressions optimization preserves semantics
        #[test]
        fn test_optimization_comparison(
            expr_str in arb_comparison_expr()
        ) {
            let mut engine_no_opt = DefaultEngine::default();
            engine_no_opt.load_builtin_module();
            engine_no_opt.set_optimization_level(OptimizationLevel::None);
            let result_no_opt = engine_no_opt.eval(&expr_str, crate::null_input().into_iter());

            let mut engine_opt = DefaultEngine::default();
            engine_opt.load_builtin_module();
            engine_opt.set_optimization_level(OptimizationLevel::Full);
            let result_opt = engine_opt.eval(&expr_str, crate::null_input().into_iter());

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Logical expressions optimization preserves semantics
        #[test]
        fn test_optimization_logical(
            expr_str in arb_logical_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Division and modulo optimization preserves semantics
        #[test]
        fn test_optimization_div_mod(
            expr_str in arb_div_mod_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Let expressions optimization preserves semantics
        #[test]
        fn test_optimization_let_expr(
            expr_str in arb_let_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Deeply nested expressions optimization preserves semantics
        #[test]
        fn test_optimization_deeply_nested(
            expr_str in arb_deeply_nested_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Mixed type expressions optimization preserves semantics
        #[test]
        fn test_optimization_mixed_type(
            expr_str in arb_mixed_type_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(val_no_opt, val_opt);
        }

        /// Property: Function definition and inlining preserves semantics
        #[test]
        fn test_optimization_function_def(
            expr_str in arb_function_def_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(
                val_no_opt,
                val_opt
            );
        }

        /// Property: Complex expressions optimization preserves semantics
        #[test]
        fn test_optimization_complex(
            expr_str in arb_any_expr()
        ) {
            let mut engine = DefaultEngine::default();
            let result_no_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::None);
            let result_opt = engine.eval_with_level(&expr_str, crate::null_input().into_iter(), OptimizationLevel::Full);

            prop_assert!(result_no_opt.is_ok());
            prop_assert!(result_opt.is_ok());

            let val_no_opt = &result_no_opt.unwrap()[0];
            let val_opt = &result_opt.unwrap()[0];

            prop_assert_eq!(
                val_no_opt,
                val_opt
            );
        }
    }

    /// Test for implicit first argument handling in function inlining
    #[test]
    fn test_inline_with_implicit_first_argument() {
        let query = r#"
def my_func(x):
  x
end
| 42 | my_func()
"#;

        let mut engine_no_opt = DefaultEngine::default();
        let result_no_opt =
            engine_no_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::None);

        let mut engine_opt = DefaultEngine::default();
        let result_opt = engine_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::Full);

        assert!(result_no_opt.is_ok(), "No optimization failed: {:?}", result_no_opt);
        assert!(result_opt.is_ok(), "Optimization failed: {:?}", result_opt);

        let val_no_opt = &result_no_opt.unwrap()[0];
        let val_opt = &result_opt.unwrap()[0];

        assert_eq!(
            val_no_opt, val_opt,
            "Results differ between optimized and non-optimized versions"
        );
    }

    /// Test for implicit first argument with builtin function call
    #[test]
    fn test_inline_with_implicit_arg_and_builtin() {
        let query = r#"
def my_split(x):
  split(x, "_")
end
| "hello_world" | my_split()
"#;

        let mut engine_no_opt = DefaultEngine::default();
        let result_no_opt =
            engine_no_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::None);

        let mut engine_opt = DefaultEngine::default();
        let result_opt = engine_opt.eval_with_level(query, crate::null_input().into_iter(), OptimizationLevel::Full);

        assert!(result_no_opt.is_ok(), "No optimization failed: {:?}", result_no_opt);
        assert!(result_opt.is_ok(), "Optimization failed: {:?}", result_opt);

        let val_no_opt = &result_no_opt.unwrap()[0];
        let val_opt = &result_opt.unwrap()[0];

        assert_eq!(
            val_no_opt, val_opt,
            "Results differ between optimized and non-optimized versions"
        );
    }
}
