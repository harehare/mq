use super::bytecode::{Chunk, OpCode, ParamBinding, ParamShape, SELF_SLOT, UpvalueSource};
use super::resolver::FunctionScope;
use crate::Shared;
use crate::ast::constants::builtins;
use crate::ast::node::{AccessTarget, Expr, Literal, Node, Pattern, StringSegment};
use crate::ast::{Program, node as ast};
use crate::eval::builtin;
use crate::eval::runtime_value::RuntimeValue;
use crate::{ModuleError, ModuleLoader, ModuleResolver, TokenArena};
use rustc_hash::{FxHashMap, FxHashSet};
use std::fmt;

#[cfg(feature = "debugger")]
use super::debug_symbols::DebugSymbolTable;

#[derive(Debug)]
pub(crate) enum CompileError {
    UndefinedIdent(String, crate::ast::TokenId),
    Unsupported(&'static str, crate::ast::TokenId),
    #[allow(dead_code)]
    UnsupportedExpr(String, crate::ast::TokenId),
    Module(ModuleError),
    AssignToImmutable(String, crate::ast::TokenId),
}

impl CompileError {
    #[cold]
    pub(crate) fn token_id(&self) -> Option<crate::ast::TokenId> {
        match self {
            CompileError::UndefinedIdent(_, token_id)
            | CompileError::Unsupported(_, token_id)
            | CompileError::UnsupportedExpr(_, token_id)
            | CompileError::AssignToImmutable(_, token_id) => Some(*token_id),
            CompileError::Module(_) => None,
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::UndefinedIdent(name, _) => write!(f, "undefined identifier `{name}` (vm M2 subset)"),
            CompileError::Unsupported(what, _) => write!(f, "unsupported in vm M2 subset: {what}"),
            CompileError::UnsupportedExpr(what, _) => write!(f, "unsupported expression in vm M2 subset: {what}"),
            CompileError::Module(e) => write!(f, "{e}"),
            CompileError::AssignToImmutable(name, _) => write!(f, "cannot assign to immutable variable \"{name}\""),
        }
    }
}

impl std::error::Error for CompileError {}

type CompileResult<T> = Result<T, CompileError>;

pub(crate) struct CompiledProgram {
    pub(crate) chunks: Shared<Vec<Chunk>>,
}

enum Resolved {
    Local(u16),
    Upvalue(u16),
}

enum ContinueTarget {
    Fixed(usize),
    Pending(Vec<usize>),
}

struct LoopCtx {
    continue_target: ContinueTarget,
    break_jumps: Vec<usize>,
    acc_slot: u16,
    chunk_index: usize,
}

struct Compiler<R: ModuleResolver> {
    chunks: Vec<Chunk>,
    scopes: Vec<FunctionScope>,
    current: usize,
    loops: Vec<LoopCtx>,
    current_token_id: crate::ast::TokenId,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    qualified_bindings: FxHashMap<(crate::Ident, crate::Ident), (usize, u16)>,
    external_globals: FxHashSet<crate::Ident>,
    used_unresolved_call_name: bool,
}

pub(crate) fn compile_program<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
) -> CompileResult<CompiledProgram> {
    compile_program_impl(program, token_arena, module_loader, false, &[], &[]).map(|(compiled, _)| compiled)
}

/// Compiles a debugger expression with paused-frame names predeclared as top-level slots.
#[cfg(feature = "debugger")]
#[allow(dead_code)] // Used by the VM debugger adapter before its Engine cutover in M5.
pub(crate) fn compile_debug_expression<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    bindings: &[crate::Ident],
) -> CompileResult<CompiledProgram> {
    compile_program_impl(program, token_arena, module_loader, false, bindings, &[]).map(|(compiled, _)| compiled)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn compile_program_with_builtin_prelude<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
) -> CompileResult<CompiledProgram> {
    compile_program_impl(program, token_arena, module_loader, true, &[], &[]).map(|(compiled, _)| compiled)
}

pub(crate) fn compile_program_for_engine<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    external_globals: &[crate::Ident],
) -> CompileResult<CompiledProgram> {
    match compile_program_impl(
        program,
        Shared::clone(&token_arena),
        module_loader.clone(),
        false,
        &[],
        external_globals,
    ) {
        Ok((compiled, false)) => Ok(compiled),
        Ok((_, true)) | Err(CompileError::UndefinedIdent(..)) => {
            compile_program_impl(program, token_arena, module_loader, true, &[], external_globals)
                .map(|(compiled, _)| compiled)
        }
        Err(other) => Err(other),
    }
}

/// Returns the compiled program plus `Compiler::used_unresolved_call_name`.
fn compile_program_impl<R: ModuleResolver>(
    program: &Program,
    token_arena: TokenArena,
    module_loader: ModuleLoader<R>,
    load_builtin_prelude: bool,
    seed_bindings: &[crate::Ident],
    external_globals: &[crate::Ident],
) -> CompileResult<(CompiledProgram, bool)> {
    let mut scope = FunctionScope::default();
    assert_eq!(scope.declare_synthetic(), SELF_SLOT, "self must be slot 0");
    for name in seed_bindings {
        scope.declare(*name);
    }
    let mut compiler = Compiler {
        chunks: vec![Chunk::default()],
        scopes: vec![scope],
        current: 0,
        loops: Vec::new(),
        current_token_id: crate::ast::TokenId::new(0),
        token_arena,
        module_loader,
        qualified_bindings: FxHashMap::default(),
        external_globals: external_globals.iter().copied().collect(),
        used_unresolved_call_name: false,
    };
    if load_builtin_prelude {
        let builtin_module = compiler
            .module_loader
            .load_builtin(Shared::clone(&compiler.token_arena))
            .map_err(CompileError::Module)?;
        compiler.compile_flattened_module(&builtin_module)?;
    }

    compiler.compile_top_level(program)?;
    compiler.emit(OpCode::Return);
    compiler.chunks[0].local_count = compiler.scopes[0].local_count();
    #[cfg(feature = "debugger")]
    {
        compiler.chunks[0].debug_symbols =
            DebugSymbolTable::new(compiler.scopes[0].debug_locals(), compiler.scopes[0].debug_upvalues());
    }
    Ok((
        CompiledProgram {
            chunks: Shared::new(compiler.chunks),
        },
        compiler.used_unresolved_call_name,
    ))
}

impl<R: ModuleResolver> Compiler<R> {
    fn compile_top_level(&mut self, program: &Program) -> CompileResult<()> {
        enum Deferred {
            Statement(Shared<Node>),
            ModuleVars(crate::Module),
            InlineModuleRest(crate::Ident, usize, Program),
        }
        let mut deferred = Vec::with_capacity(program.len());
        let mut defs: Program = Vec::new();
        for node in program {
            self.current_token_id = node.token_id;
            match &*node.expr {
                Expr::Def(_, _, _) => defs.push(Shared::clone(node)),
                Expr::Let(Pattern::Ident(ident), _) | Expr::Var(Pattern::Ident(ident), _) => {
                    self.scope_mut().declare(ident.name);
                    deferred.push(Deferred::Statement(Shared::clone(node)));
                }
                Expr::Include(literal) => {
                    let module = self.compile_include_functions(literal)?;
                    deferred.push(Deferred::ModuleVars(module));
                }
                Expr::Import(literal, alias) => {
                    let module = self.compile_import_functions(literal, alias.as_ref())?;
                    deferred.push(Deferred::ModuleVars(module));
                }
                Expr::Module(ident, inline_program) => {
                    let module_alias = ident.name;
                    let depth = self.scopes.len() - 1;
                    let rest = self.compile_module_functions(ident, inline_program)?;
                    deferred.push(Deferred::InlineModuleRest(module_alias, depth, rest));
                }
                _ => deferred.push(Deferred::Statement(Shared::clone(node))),
            }
        }
        self.compile_functions_with_forward_refs(&defs)?;
        for item in deferred {
            match item {
                Deferred::Statement(node) => {
                    self.compile_expr(&node)?;
                    if Self::is_auto_call_candidate(&node) {
                        self.emit(OpCode::MaybeAutoCall);
                    }
                }
                Deferred::ModuleVars(module) => self.compile_module_vars(&module)?,
                Deferred::InlineModuleRest(module_alias, depth, rest) => {
                    self.compile_module_rest(module_alias, depth, &rest)?
                }
            }
            self.emit(OpCode::SetLocal(SELF_SLOT));
        }
        self.emit(OpCode::GetLocal(SELF_SLOT));
        Ok(())
    }

    fn chunk_mut(&mut self) -> &mut Chunk {
        &mut self.chunks[self.current]
    }

    fn scope_mut(&mut self) -> &mut FunctionScope {
        self.scopes.last_mut().expect("at least one scope")
    }

    fn emit(&mut self, op: OpCode) -> usize {
        let token_id = self.current_token_id;
        self.chunk_mut().emit(op, token_id)
    }

    fn compile_body(&mut self, body: &Program) -> CompileResult<()> {
        for node in body {
            if let Expr::Def(ident, _, _) = &*node.expr {
                self.scope_mut().declare(ident.name);
            }
        }
        for node in body {
            self.compile_expr(node)?;
            if Self::is_auto_call_candidate(node) {
                self.emit(OpCode::MaybeAutoCall);
            }
            self.emit(OpCode::SetLocal(SELF_SLOT));
        }
        self.emit(OpCode::GetLocal(SELF_SLOT));
        Ok(())
    }

    fn is_auto_call_candidate(node: &Node) -> bool {
        matches!(
            &*node.expr,
            Expr::Ident(_) | Expr::QualifiedAccess(_, AccessTarget::Ident(_))
        )
    }

    fn compile_function(
        &mut self,
        params: &ast::Params,
        body: &Program,
        name_for_shadow: Option<crate::Ident>,
    ) -> CompileResult<(u16, Vec<UpvalueSource>)> {
        let outer = self.current;
        self.chunks.push(Chunk::default());
        let new_index = (self.chunks.len() - 1) as u16;
        self.current = new_index as usize;

        let mut scope = FunctionScope::default();
        assert_eq!(scope.declare_synthetic(), SELF_SLOT, "self must be slot 0");
        if let Some(name) = name_for_shadow
            && builtin::get_builtin_functions(&name).is_some()
        {
            scope.shadowed_builtin = Some(name);
        }
        let param_slots: Vec<u16> = params.iter().map(|param| scope.declare(param.ident.name)).collect();
        self.scopes.push(scope);

        let mut bindings = Vec::with_capacity(params.len());
        let mut required = 0usize;
        let mut has_variadic = false;
        for (param, &slot) in params.iter().zip(&param_slots) {
            if param.is_variadic {
                has_variadic = true;
                bindings.push(ParamBinding::Variadic(slot));
            } else if let Some(default_expr) = &param.default {
                let default_body: Program = vec![Shared::clone(default_expr)];
                let (default_chunk, default_upvalues) =
                    self.compile_function(&ast::Params::new(), &default_body, None)?;
                bindings.push(ParamBinding::Optional(slot, default_chunk, default_upvalues));
            } else {
                required += 1;
                bindings.push(ParamBinding::Required(slot));
            }
        }

        self.compile_body(body)?;
        self.emit(OpCode::Return);

        let finished = self.scopes.pop().expect("scope pushed above");
        self.chunks[new_index as usize].local_count = finished.local_count();
        self.chunks[new_index as usize].param_shape = ParamShape {
            bindings,
            required,
            has_variadic,
        };
        #[cfg(feature = "debugger")]
        {
            self.chunks[new_index as usize].debug_symbols =
                DebugSymbolTable::new(finished.debug_locals(), finished.debug_upvalues());
        }
        let upvalues = finished.upvalue_sources();

        self.current = outer;
        Ok((new_index, upvalues))
    }

    fn compile_try(
        &mut self,
        body: &Shared<Node>,
        binder: &Option<ast::IdentWithToken>,
        catch: &Shared<Node>,
    ) -> CompileResult<()> {
        let try_program: Program = vec![Shared::clone(body)];
        let (try_chunk, try_upvalues) = self.compile_function(&ast::Params::new(), &try_program, None)?;

        let mut catch_params = ast::Params::new();
        if let Some(binder) = binder {
            catch_params.push(ast::Param::new(binder.clone()));
        }
        let catch_program: Program = vec![Shared::clone(catch)];
        let (catch_chunk, catch_upvalues) = self.compile_function(&catch_params, &catch_program, None)?;

        self.emit(OpCode::MakeClosure(try_chunk, try_upvalues));
        self.emit(OpCode::MakeClosure(catch_chunk, catch_upvalues));
        self.emit(OpCode::TryCatch(binder.is_some()));
        Ok(())
    }

    fn compile_let_or_var(&mut self, pattern: &Pattern, value: &Shared<Node>, mutable: bool) -> CompileResult<()> {
        match pattern {
            Pattern::Ident(ident) if matches!(&*value.expr, Expr::Fn(_, _)) => {
                let Expr::Fn(params, body) = &*value.expr else {
                    unreachable!("guarded above");
                };
                let slot = self.scope_mut().declare_or_reuse(ident.name);
                if mutable {
                    self.scope_mut().unmark_immutable(slot);
                } else {
                    self.scope_mut().mark_immutable(slot);
                }
                let (chunk_idx, upvalues) = self.compile_function(params, body, Some(ident.name))?;
                self.emit(OpCode::MakeClosure(chunk_idx, upvalues));
                self.emit(OpCode::SetLocal(slot));
            }
            Pattern::Ident(ident) => {
                self.compile_expr(value)?;
                let slot = self.scope_mut().declare_or_reuse(ident.name);
                if mutable {
                    self.scope_mut().unmark_immutable(slot);
                } else {
                    self.scope_mut().mark_immutable(slot);
                }
                self.emit(OpCode::SetLocal(slot));
            }
            _ => {
                let subject_slot = self.scope_mut().declare_synthetic();
                self.compile_expr(value)?;
                self.emit(OpCode::SetLocal(subject_slot));

                let locals_before = self.scope_mut().local_count();
                let mut fail_jumps = Vec::new();
                self.compile_pattern_test(pattern, subject_slot, &mut fail_jumps)?;
                if !mutable {
                    for slot in locals_before..self.scope_mut().local_count() {
                        self.scope_mut().mark_immutable(slot);
                    }
                }

                let end_jump = self.emit(OpCode::Jump(0));
                for jump in fail_jumps {
                    self.chunk_mut().patch_jump(jump);
                }
                self.emit(OpCode::RaiseDestructuringFailed);
                self.chunk_mut().patch_jump(end_jump);
            }
        }
        self.emit(OpCode::GetLocal(SELF_SLOT));
        Ok(())
    }

    fn compile_match(&mut self, subject: &Shared<Node>, arms: &ast::MatchArms) -> CompileResult<()> {
        let subject_slot = self.scope_mut().declare_synthetic();
        self.compile_expr(subject)?;
        self.emit(OpCode::SetLocal(subject_slot));

        let mut end_jumps = Vec::with_capacity(arms.len());
        for arm in arms {
            let mut fail_jumps = Vec::new();
            self.compile_pattern_test(&arm.pattern, subject_slot, &mut fail_jumps)?;
            if let Some(guard) = &arm.guard {
                self.compile_expr(guard)?;
                fail_jumps.push(self.emit(OpCode::JumpIfFalse(0)));
            }
            self.compile_expr(&arm.body)?;
            end_jumps.push(self.emit(OpCode::Jump(0)));
            for jump in fail_jumps {
                self.chunk_mut().patch_jump(jump);
            }
        }
        self.emit(OpCode::PushNone);
        for jump in end_jumps {
            self.chunk_mut().patch_jump(jump);
        }
        Ok(())
    }

    fn compile_pattern_test(
        &mut self,
        pattern: &Pattern,
        subject_slot: u16,
        fail_jumps: &mut Vec<usize>,
    ) -> CompileResult<()> {
        match pattern {
            Pattern::Wildcard => {}
            Pattern::Ident(ident) => {
                let slot = self.scope_mut().declare(ident.name);
                self.emit(OpCode::GetLocal(subject_slot));
                self.emit(OpCode::SetLocal(slot));
            }
            Pattern::Literal(lit) => {
                self.emit(OpCode::GetLocal(subject_slot));
                let value = literal_to_runtime_value(lit);
                let idx = self.chunk_mut().push_const(value);
                self.emit(OpCode::Const(idx));
                self.emit(OpCode::Eq);
                fail_jumps.push(self.emit(OpCode::JumpIfFalse(0)));
            }
            Pattern::Type(type_ident) => {
                self.emit(OpCode::GetLocal(subject_slot));
                self.emit(OpCode::TypeCheck(*type_ident));
                fail_jumps.push(self.emit(OpCode::JumpIfFalse(0)));
            }
            Pattern::Array(elems) => {
                self.emit_array_shape_check(subject_slot, elems.len(), OpCode::Eq, fail_jumps);
                for (i, sub) in elems.iter().enumerate() {
                    self.compile_array_elem_test(subject_slot, i, sub, fail_jumps)?;
                }
            }
            Pattern::ArrayRest(elems, rest_ident) => {
                self.emit_array_shape_check(subject_slot, elems.len(), OpCode::Ge, fail_jumps);
                for (i, sub) in elems.iter().enumerate() {
                    self.compile_array_elem_test(subject_slot, i, sub, fail_jumps)?;
                }
                let rest_slot = self.scope_mut().declare(rest_ident.name);
                self.emit(OpCode::GetLocal(subject_slot));
                let offset = self
                    .chunk_mut()
                    .push_const(RuntimeValue::Number((elems.len() as f64).into()));
                self.emit(OpCode::Const(offset));
                self.emit(OpCode::ArraySliceFrom);
                self.emit(OpCode::SetLocal(rest_slot));
            }
            Pattern::Or(alts) => {
                let mut success_jumps = Vec::with_capacity(alts.len().saturating_sub(1));
                for (i, alt) in alts.iter().enumerate() {
                    let mut local_fails = Vec::new();
                    self.compile_pattern_test(alt, subject_slot, &mut local_fails)?;
                    if i + 1 < alts.len() {
                        success_jumps.push(self.emit(OpCode::Jump(0)));
                        for f in local_fails {
                            self.chunk_mut().patch_jump(f);
                        }
                    } else {
                        fail_jumps.extend(local_fails);
                    }
                }
                for s in success_jumps {
                    self.chunk_mut().patch_jump(s);
                }
            }
            Pattern::Dict(entries) => {
                self.emit(OpCode::GetLocal(subject_slot));
                self.emit(OpCode::TypeCheck(builtins::DICT.into()));
                fail_jumps.push(self.emit(OpCode::JumpIfFalse(0)));

                for (key, sub_pattern) in entries {
                    let value_slot = self.scope_mut().declare_synthetic();
                    self.emit(OpCode::GetLocal(subject_slot));
                    let key_idx = self.chunk_mut().push_const(RuntimeValue::Symbol(key.name));
                    self.emit(OpCode::Const(key_idx));
                    self.emit(OpCode::CallBuiltin(builtins::GET.into(), 2));
                    self.emit(OpCode::SetLocal(value_slot));

                    // `get` on a missing key also returns `None`, so a key mapped to a
                    // real `None` value is indistinguishable from an absent one here.
                    self.emit(OpCode::GetLocal(value_slot));
                    self.emit(OpCode::TypeCheck("none".into()));
                    let has_value = self.emit(OpCode::JumpIfFalse(0));
                    fail_jumps.push(self.emit(OpCode::Jump(0)));
                    self.chunk_mut().patch_jump(has_value);

                    self.compile_pattern_test(sub_pattern, value_slot, fail_jumps)?;
                }
            }
        }
        Ok(())
    }

    fn emit_array_shape_check(&mut self, subject_slot: u16, min_len: usize, cmp: OpCode, fail_jumps: &mut Vec<usize>) {
        self.emit(OpCode::GetLocal(subject_slot));
        self.emit(OpCode::TypeCheck(builtins::ARRAY.into()));
        fail_jumps.push(self.emit(OpCode::JumpIfFalse(0)));

        self.emit(OpCode::GetLocal(subject_slot));
        self.emit(OpCode::ArrayLen);
        let len_idx = self
            .chunk_mut()
            .push_const(RuntimeValue::Number((min_len as f64).into()));
        self.emit(OpCode::Const(len_idx));
        self.emit(cmp);
        fail_jumps.push(self.emit(OpCode::JumpIfFalse(0)));
    }

    fn compile_array_elem_test(
        &mut self,
        subject_slot: u16,
        index: usize,
        pattern: &Pattern,
        fail_jumps: &mut Vec<usize>,
    ) -> CompileResult<()> {
        let elem_slot = self.scope_mut().declare_synthetic();
        self.emit(OpCode::GetLocal(subject_slot));
        let idx_const = self.chunk_mut().push_const(RuntimeValue::Number((index as f64).into()));
        self.emit(OpCode::Const(idx_const));
        self.emit(OpCode::ArrayGetAt);
        self.emit(OpCode::SetLocal(elem_slot));
        self.compile_pattern_test(pattern, elem_slot, fail_jumps)
    }

    fn compile_interpolated_string(&mut self, segments: &[StringSegment]) -> CompileResult<()> {
        for segment in segments {
            match segment {
                StringSegment::Text(s) => {
                    let idx = self.chunk_mut().push_const(RuntimeValue::String(s.clone()));
                    self.emit(OpCode::Const(idx));
                }
                StringSegment::Expr(node) => self.compile_expr(node)?,
                StringSegment::Env(name) => {
                    let idx = self.chunk_mut().push_const(RuntimeValue::String(name.to_string()));
                    self.emit(OpCode::GetEnvVar(idx));
                }
                StringSegment::Self_ => {
                    self.emit(OpCode::GetLocal(SELF_SLOT));
                }
            }
        }
        self.emit(OpCode::InterpString(segments.len() as u16));
        Ok(())
    }

    fn compile_include(&mut self, literal: &Literal) -> CompileResult<()> {
        let module = self.compile_include_functions(literal)?;
        self.compile_module_vars(&module)
    }

    fn compile_include_functions(&mut self, literal: &Literal) -> CompileResult<crate::Module> {
        let Literal::String(path) = literal else {
            return Err(CompileError::Unsupported(
                "include target must be a string literal",
                self.current_token_id,
            ));
        };
        let module = self.load_module_or_reload(path)?;
        self.compile_discarding(&module.modules)?;
        self.predeclare_module_var_slots(&module.vars);
        self.compile_functions_with_forward_refs(&module.functions)?;
        Ok(module)
    }

    fn load_module_or_reload(&mut self, path: &str) -> CompileResult<crate::Module> {
        match self
            .module_loader
            .load_from_file(path, Shared::clone(&self.token_arena))
        {
            Ok(module) => Ok(module),
            Err(ModuleError::AlreadyLoaded(_)) => self
                .module_loader
                .reload_cached(path, Shared::clone(&self.token_arena))
                .map_err(CompileError::Module),
            Err(e) => Err(CompileError::Module(e)),
        }
    }

    fn predeclare_module_var_slots(&mut self, vars: &Program) {
        for node in vars {
            if let Expr::Let(Pattern::Ident(ident), _) = &*node.expr {
                self.scope_mut().declare(ident.name);
            }
        }
    }

    fn compile_module_vars(&mut self, module: &crate::Module) -> CompileResult<()> {
        self.compile_discarding(&module.vars)?;
        self.emit(OpCode::GetLocal(SELF_SLOT));
        Ok(())
    }

    fn compile_discarding(&mut self, nodes: &Program) -> CompileResult<()> {
        for node in nodes {
            self.compile_expr(node)?;
            self.emit(OpCode::Pop);
        }
        Ok(())
    }

    fn compile_flattened_module(&mut self, module: &crate::Module) -> CompileResult<()> {
        self.compile_discarding(&module.modules)?;
        self.compile_functions_with_forward_refs(&module.functions)?;
        self.compile_discarding(&module.vars)?;
        Ok(())
    }

    fn compile_functions_with_forward_refs(&mut self, nodes: &Program) -> CompileResult<()> {
        let mut slots = Vec::with_capacity(nodes.len());
        for node in nodes {
            let Expr::Def(ident, _, _) = &*node.expr else {
                return Err(CompileError::Unsupported(
                    "module top-level statement is not a def",
                    self.current_token_id,
                ));
            };
            slots.push(self.scope_mut().declare(ident.name));
        }
        for (node, slot) in nodes.iter().zip(slots) {
            self.current_token_id = node.token_id;
            let Expr::Def(ident, params, body) = &*node.expr else {
                unreachable!("validated as Def above");
            };
            let (chunk_idx, upvalues) = self.compile_function(params, body, Some(ident.name))?;
            self.emit(OpCode::MakeClosure(chunk_idx, upvalues));
            self.emit(OpCode::SetLocal(slot));
        }
        Ok(())
    }

    fn compile_import(&mut self, literal: &Literal, alias: Option<&ast::IdentWithToken>) -> CompileResult<()> {
        let module = self.compile_import_functions(literal, alias)?;
        self.compile_module_vars(&module)
    }

    fn compile_import_functions(
        &mut self,
        literal: &Literal,
        alias: Option<&ast::IdentWithToken>,
    ) -> CompileResult<crate::Module> {
        let Literal::String(path) = literal else {
            return Err(CompileError::Unsupported(
                "import target must be a string literal",
                self.current_token_id,
            ));
        };
        let module = self.load_module_or_reload(path)?;
        let module_alias = alias.map(|a| a.name).unwrap_or_else(|| crate::Ident::new(&module.name));

        self.compile_discarding(&module.modules)?;
        self.predeclare_module_var_slots(&module.vars);

        let depth = self.scopes.len() - 1;
        let mut slots = Vec::with_capacity(module.functions.len());
        for node in &module.functions {
            let Expr::Def(ident, _, _) = &*node.expr else {
                return Err(CompileError::Unsupported(
                    "module function is not a def",
                    self.current_token_id,
                ));
            };
            slots.push(self.scope_mut().declare(ident.name));
        }
        for (node, slot) in module.functions.iter().zip(&slots) {
            self.current_token_id = node.token_id;
            let Expr::Def(ident, params, body) = &*node.expr else {
                unreachable!("validated as Def above");
            };
            let (chunk_idx, upvalues) = self.compile_function(params, body, Some(ident.name))?;
            self.emit(OpCode::MakeClosure(chunk_idx, upvalues));
            self.emit(OpCode::SetLocal(*slot));
            self.qualified_bindings
                .insert((module_alias, ident.name), (depth, *slot));
        }
        // Hide the real names now that every body referencing them has been compiled, so
        // only `alias::name` — not the bare name — resolves to them from here on.
        for slot in slots {
            self.scope_mut().set_local_name(slot, crate::Ident::default());
        }
        Ok(module)
    }

    fn compile_module(&mut self, ident: &ast::IdentWithToken, program: &Program) -> CompileResult<()> {
        let module_alias = ident.name;
        let depth = self.scopes.len() - 1;
        let rest = self.compile_module_functions(ident, program)?;
        self.compile_module_rest(module_alias, depth, &rest)
    }

    fn compile_module_functions(&mut self, ident: &ast::IdentWithToken, program: &Program) -> CompileResult<Program> {
        let module_alias = ident.name;
        let depth = self.scopes.len() - 1;

        let mut def_slots = FxHashMap::default();
        for node in program {
            if let Expr::Def(def_ident, _, _) = &*node.expr {
                def_slots.insert(def_ident.name, self.scope_mut().declare(def_ident.name));
            }
        }

        let mut rest = Program::new();
        for node in program {
            self.current_token_id = node.token_id;
            match &*node.expr {
                Expr::Def(def_ident, params, body) => {
                    let slot = def_slots[&def_ident.name];
                    let (chunk_idx, upvalues) = self.compile_function(params, body, Some(def_ident.name))?;
                    self.emit(OpCode::MakeClosure(chunk_idx, upvalues));
                    self.emit(OpCode::SetLocal(slot));
                    self.qualified_bindings
                        .insert((module_alias, def_ident.name), (depth, slot));
                }
                Expr::Include(_) | Expr::Let(_, _) | Expr::Import(_, _) | Expr::Module(_, _) => {
                    rest.push(Shared::clone(node));
                }
                _ => {}
            }
        }

        for slot in def_slots.values() {
            self.scope_mut().set_local_name(*slot, crate::Ident::default());
        }
        Ok(rest)
    }

    fn compile_module_rest(&mut self, module_alias: crate::Ident, depth: usize, rest: &Program) -> CompileResult<()> {
        for node in rest {
            self.current_token_id = node.token_id;
            match &*node.expr {
                Expr::Include(literal) => {
                    self.compile_include(literal)?;
                    self.emit(OpCode::Pop);
                }
                Expr::Let(Pattern::Ident(let_ident), value) => {
                    self.compile_expr(value)?;
                    let slot = self.scope_mut().declare_synthetic();
                    self.emit(OpCode::SetLocal(slot));
                    self.qualified_bindings
                        .insert((module_alias, let_ident.name), (depth, slot));
                }
                Expr::Let(_, _) => {
                    return Err(CompileError::Unsupported(
                        "destructuring let inside an inline `module` block",
                        self.current_token_id,
                    ));
                }
                Expr::Import(literal, alias) => {
                    self.compile_import(literal, alias.as_ref())?;
                    self.emit(OpCode::Pop);
                }
                Expr::Module(nested_ident, nested_program) => {
                    self.compile_module(nested_ident, nested_program)?;
                }
                _ => {}
            }
        }
        self.emit(OpCode::GetLocal(SELF_SLOT));
        Ok(())
    }

    fn compile_qualified_access(&mut self, path: &[ast::IdentWithToken], target: &AccessTarget) -> CompileResult<()> {
        let [alias] = path else {
            return Err(CompileError::Unsupported(
                "qualified access deeper than one alias segment",
                self.current_token_id,
            ));
        };
        let member = match target {
            AccessTarget::Ident(id) => id.name,
            AccessTarget::Call(id, _) => id.name,
        };
        let (depth, slot) = *self
            .qualified_bindings
            .get(&(alias.name, member))
            .ok_or_else(|| CompileError::UndefinedIdent(format!("{}::{member}", alias.name), self.current_token_id))?;
        if depth >= self.scopes.len() {
            return Err(CompileError::Unsupported(
                "qualified access from a shallower scope than its import",
                self.current_token_id,
            ));
        }
        match self.resolve_qualified_slot(self.scopes.len() - 1, depth, slot) {
            Resolved::Local(slot) => self.emit(OpCode::GetLocal(slot)),
            Resolved::Upvalue(idx) => self.emit(OpCode::GetUpvalue(idx)),
        };
        if let AccessTarget::Call(_, args) = target {
            for arg in args {
                self.compile_expr(arg)?;
            }
            self.emit(OpCode::CallValue(args.len() as u8));
        }
        Ok(())
    }

    fn resolve_qualified_slot(&mut self, depth: usize, declared_depth: usize, slot: u16) -> Resolved {
        if depth == declared_depth {
            return Resolved::Local(slot);
        }
        match self.resolve_qualified_slot(depth - 1, declared_depth, slot) {
            Resolved::Local(inner_slot) => {
                let idx = self.scopes[depth].add_upvalue_for_source(UpvalueSource::Local(inner_slot));
                Resolved::Upvalue(idx)
            }
            Resolved::Upvalue(inner_idx) => {
                let idx = self.scopes[depth].add_upvalue_for_source(UpvalueSource::Upvalue(inner_idx));
                Resolved::Upvalue(idx)
            }
        }
    }

    fn resolve(&mut self, name: crate::Ident) -> Option<Resolved> {
        self.resolve_at(self.scopes.len() - 1, name)
    }

    fn resolve_at(&mut self, depth: usize, name: crate::Ident) -> Option<Resolved> {
        if let Some(slot) = self.scopes[depth].resolve_local(name) {
            return Some(Resolved::Local(slot));
        }
        if depth == 0 {
            return None;
        }
        match self.resolve_at(depth - 1, name)? {
            Resolved::Local(slot) => {
                let idx = self.scopes[depth].add_upvalue(name, UpvalueSource::Local(slot));
                Some(Resolved::Upvalue(idx))
            }
            Resolved::Upvalue(up_idx) => {
                let idx = self.scopes[depth].add_upvalue(name, UpvalueSource::Upvalue(up_idx));
                Some(Resolved::Upvalue(idx))
            }
        }
    }

    fn compile_expr(&mut self, node: &Shared<Node>) -> CompileResult<()> {
        self.current_token_id = node.token_id;
        #[cfg(feature = "debugger")]
        {
            self.chunk_mut().debug_nodes.push((node.token_id, Shared::clone(node)));
            self.emit(OpCode::StmtBoundary(node.token_id));
        }
        match &*node.expr {
            Expr::Literal(lit) => {
                let value = literal_to_runtime_value(lit);
                let idx = self.chunk_mut().push_const(value);
                self.emit(OpCode::Const(idx));
                Ok(())
            }
            Expr::Ident(ident) => self.compile_ident_get(ident.name),
            Expr::Let(pattern, value) => self.compile_let_or_var(pattern, value, false),
            Expr::Var(pattern, value) => self.compile_let_or_var(pattern, value, true),
            Expr::Assign(ident, value) => {
                self.compile_expr(value)?;
                match self.resolve(ident.name) {
                    Some(Resolved::Local(slot)) => {
                        if self.scope_mut().is_immutable(slot) {
                            return Err(CompileError::AssignToImmutable(
                                ident.name.to_string(),
                                self.current_token_id,
                            ));
                        }
                        self.emit(OpCode::SetLocal(slot));
                        self.emit(OpCode::GetLocal(SELF_SLOT));
                    }
                    Some(Resolved::Upvalue(idx)) => {
                        self.emit(OpCode::SetUpvalue(idx));
                        self.emit(OpCode::GetLocal(SELF_SLOT));
                    }
                    None => {
                        return Err(CompileError::UndefinedIdent(
                            ident.name.to_string(),
                            self.current_token_id,
                        ));
                    }
                }
                Ok(())
            }
            Expr::Def(ident, params, body) => {
                let slot = self.scope_mut().declare_or_reuse(ident.name);
                let (chunk_idx, upvalues) = self.compile_function(params, body, Some(ident.name))?;
                self.emit(OpCode::MakeClosure(chunk_idx, upvalues));
                self.emit(OpCode::SetLocal(slot));
                self.emit(OpCode::GetLocal(slot));
                Ok(())
            }
            Expr::Fn(params, body) => {
                let (chunk_idx, upvalues) = self.compile_function(params, body, None)?;
                self.emit(OpCode::MakeClosure(chunk_idx, upvalues));
                Ok(())
            }
            Expr::As(ident, value) => {
                self.compile_expr(value)?;
                let slot = self.scope_mut().declare(ident.name);
                self.emit(OpCode::SetLocal(slot));
                self.emit(OpCode::GetLocal(SELF_SLOT));
                Ok(())
            }
            Expr::Call(ident, args) => self.compile_call(ident.name, args),
            Expr::CallDynamic(callee, args) => {
                self.compile_expr(callee)?;
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(OpCode::CallValue(args.len() as u8));
                Ok(())
            }
            Expr::Block(body) => self.compile_body(body),
            Expr::If(branches) => self.compile_if(branches),
            Expr::Unless(branches) => self.compile_unless(branches),
            Expr::While(cond, body) => self.compile_while(cond, body),
            Expr::Until(cond, body) => self.compile_until(cond, body),
            Expr::Loop(body) => self.compile_loop(body),
            Expr::Break(value) => {
                let loop_ctx = self
                    .loops
                    .last()
                    .ok_or(CompileError::Unsupported("break outside a loop", self.current_token_id))?;
                if loop_ctx.chunk_index != self.current {
                    return Err(CompileError::Unsupported(
                        "break cannot cross into a nested chunk (a try/catch body)",
                        self.current_token_id,
                    ));
                }
                let acc_slot = loop_ctx.acc_slot;
                if let Some(v) = value {
                    self.compile_expr(v)?;
                    self.emit(OpCode::SetLocal(acc_slot));
                }
                let at = self.emit(OpCode::Jump(0));
                self.loops.last_mut().unwrap().break_jumps.push(at);
                Ok(())
            }
            Expr::Continue => {
                let loop_ctx = self.loops.last().ok_or(CompileError::Unsupported(
                    "continue outside a loop",
                    self.current_token_id,
                ))?;
                if loop_ctx.chunk_index != self.current {
                    // See `Expr::Break`'s comment above — same reasoning, same fix.
                    return Err(CompileError::Unsupported(
                        "continue cannot cross into a nested chunk (a try/catch body)",
                        self.current_token_id,
                    ));
                }
                let fixed_target = match &loop_ctx.continue_target {
                    ContinueTarget::Fixed(target) => Some(*target),
                    ContinueTarget::Pending(_) => None,
                };
                match fixed_target {
                    Some(target) => {
                        let offset = self.chunk_mut().backward_offset(target);
                        self.emit(OpCode::Jump(offset));
                    }
                    None => {
                        let at = self.emit(OpCode::Jump(0));
                        match &mut self.loops.last_mut().unwrap().continue_target {
                            ContinueTarget::Pending(sites) => sites.push(at),
                            ContinueTarget::Fixed(_) => unreachable!(),
                        }
                    }
                }
                Ok(())
            }
            Expr::Foreach(ident, iterable, body) => self.compile_foreach(ident.name, iterable, body),
            Expr::Try(body, binder, catch) => self.compile_try(body, binder, catch),
            Expr::Match(subject, arms) => self.compile_match(subject, arms),
            Expr::InterpolatedString(segments) => self.compile_interpolated_string(segments),
            Expr::Include(literal) => self.compile_include(literal),
            Expr::Import(literal, alias) => self.compile_import(literal, alias.as_ref()),
            Expr::Module(ident, program) => self.compile_module(ident, program),
            Expr::QualifiedAccess(path, target) => self.compile_qualified_access(path, target),
            Expr::Self_ | Expr::Nodes => {
                self.emit(OpCode::GetLocal(SELF_SLOT));
                Ok(())
            }
            Expr::Selector(selector) => {
                self.emit(OpCode::GetLocal(SELF_SLOT));
                self.emit(OpCode::SelectorMatch(selector.clone()));
                Ok(())
            }
            Expr::SelectorChain(selectors) => {
                self.emit(OpCode::GetLocal(SELF_SLOT));
                for selector in selectors {
                    self.emit(OpCode::SelectorMatch(selector.clone()));
                }
                Ok(())
            }
            Expr::SelectorCall(selector, args) => {
                self.emit(OpCode::GetLocal(SELF_SLOT));
                for arg in args {
                    self.compile_expr(arg)?;
                }
                self.emit(OpCode::SelectorMatchWithArgs(selector.clone(), args.len() as u8));
                Ok(())
            }
            Expr::Paren(inner) => self.compile_expr(inner),
            Expr::And(operands) => self.compile_and(operands),
            Expr::Or(operands) => self.compile_or(operands),
        }
    }

    fn compile_ident_get(&mut self, name: crate::Ident) -> CompileResult<()> {
        match self.resolve(name) {
            Some(Resolved::Local(slot)) => {
                self.emit(OpCode::GetLocal(slot));
                Ok(())
            }
            Some(Resolved::Upvalue(idx)) => {
                self.emit(OpCode::GetUpvalue(idx));
                Ok(())
            }
            None if self.external_globals.contains(&name) => {
                self.emit(OpCode::GetExternalGlobal(name));
                Ok(())
            }
            None if builtin::get_builtin_functions(&name).is_some() => {
                let idx = self.chunk_mut().push_const(RuntimeValue::NativeFunction(name));
                self.emit(OpCode::Const(idx));
                Ok(())
            }
            None => Err(CompileError::UndefinedIdent(name.to_string(), self.current_token_id)),
        }
    }

    fn compile_call(&mut self, ident: crate::Ident, args: &ast::Args) -> CompileResult<()> {
        let call_token_id = self.current_token_id;
        let shadowed = self.scope_mut().shadowed_builtin == Some(ident);

        if !shadowed && let Some(resolved) = self.resolve(ident) {
            match resolved {
                Resolved::Local(slot) => {
                    self.emit(OpCode::GetLocal(slot));
                }
                Resolved::Upvalue(idx) => {
                    self.emit(OpCode::GetUpvalue(idx));
                }
            }
            for arg in args {
                self.compile_expr(arg)?;
            }
            self.current_token_id = call_token_id;
            self.emit(OpCode::CallValue(args.len() as u8));
            return Ok(());
        }

        if args.len() == 2
            && let Some(op) = fast_path_binop(&ident)
        {
            self.compile_expr(&args[0])?;
            self.compile_expr(&args[1])?;
            self.current_token_id = call_token_id;
            self.emit(op);
            return Ok(());
        }
        if args.len() == 1 && ident == builtins::NEGATE.into() {
            self.compile_expr(&args[0])?;
            self.current_token_id = call_token_id;
            self.emit(OpCode::Neg);
            return Ok(());
        }
        if ident == builtins::ARRAY.into() {
            return self.compile_array_call(args);
        }
        if ident == builtins::DICT.into() {
            return self.compile_dict_call(args, call_token_id);
        }

        // Might be a soft prelude builtin — can't tell without the prelude loaded.
        if builtin::get_builtin_functions(&ident).is_none() {
            self.used_unresolved_call_name = true;
        }

        for arg in args {
            self.compile_expr(arg)?;
        }
        self.current_token_id = call_token_id;
        self.emit(OpCode::CallBuiltin(ident, args.len() as u8));
        Ok(())
    }

    fn is_spread(arg: &Node) -> bool {
        matches!(&*arg.expr, Expr::Call(spread_ident, _) if spread_ident.name == builtins::SPREAD.into())
    }

    fn compile_array_call(&mut self, args: &ast::Args) -> CompileResult<()> {
        self.emit(OpCode::ArrayNew);
        for arg in args {
            if let Expr::Call(spread_ident, spread_args) = &*arg.expr
                && spread_ident.name == builtins::SPREAD.into()
            {
                self.compile_expr(&spread_args[0])?;
                self.emit(OpCode::ArraySpread);
            } else {
                self.compile_expr(arg)?;
                self.emit(OpCode::ArrayPush);
            }
        }
        Ok(())
    }

    fn compile_dict_call(&mut self, args: &ast::Args, call_token_id: crate::ast::TokenId) -> CompileResult<()> {
        if !args.iter().any(|arg| Self::is_spread(arg)) {
            for arg in args {
                self.compile_expr(arg)?;
            }
            self.current_token_id = call_token_id;
            self.emit(OpCode::CallBuiltin(builtins::DICT.into(), args.len() as u8));
            return Ok(());
        }
        self.emit(OpCode::ArrayNew);
        for arg in args {
            if let Expr::Call(spread_ident, spread_args) = &*arg.expr
                && spread_ident.name == builtins::SPREAD.into()
            {
                self.compile_expr(&spread_args[0])?;
                self.emit(OpCode::DictSpread);
            } else {
                self.compile_expr(arg)?;
                self.emit(OpCode::ArrayPush);
            }
        }
        self.current_token_id = call_token_id;
        self.emit(OpCode::CallBuiltin(builtins::DICT.into(), 1));
        Ok(())
    }

    fn compile_and(&mut self, operands: &[Shared<Node>]) -> CompileResult<()> {
        if operands.is_empty() {
            let idx = self.chunk_mut().push_const(RuntimeValue::Boolean(true));
            self.emit(OpCode::Const(idx));
            return Ok(());
        }

        let last = operands.len() - 1;
        let mut false_jumps = Vec::with_capacity(operands.len());
        for (i, operand) in operands.iter().enumerate() {
            self.compile_expr(operand)?;
            self.emit(OpCode::Dup);
            false_jumps.push(self.emit(OpCode::JumpIfFalse(0)));
            if i != last {
                self.emit(OpCode::Pop);
            }
            // On the last operand, a truthy result falls through with its own
            // (undupped) copy left on the stack as the overall value.
        }
        let success_jump = self.emit(OpCode::Jump(0));

        for jump in false_jumps {
            self.chunk_mut().patch_jump(jump);
        }
        self.emit(OpCode::Pop);
        let idx = self.chunk_mut().push_const(RuntimeValue::Boolean(false));
        self.emit(OpCode::Const(idx));

        self.chunk_mut().patch_jump(success_jump);
        Ok(())
    }

    fn compile_or(&mut self, operands: &[Shared<Node>]) -> CompileResult<()> {
        if operands.is_empty() {
            let idx = self.chunk_mut().push_const(RuntimeValue::Boolean(false));
            self.emit(OpCode::Const(idx));
            return Ok(());
        }

        let mut true_jumps = Vec::with_capacity(operands.len());
        for operand in operands {
            self.compile_expr(operand)?;
            self.emit(OpCode::Dup);
            let false_jump = self.emit(OpCode::JumpIfFalse(0));
            true_jumps.push(self.emit(OpCode::Jump(0)));
            self.chunk_mut().patch_jump(false_jump);
            self.emit(OpCode::Pop);
        }
        let idx = self.chunk_mut().push_const(RuntimeValue::Boolean(false));
        self.emit(OpCode::Const(idx));

        for jump in true_jumps {
            self.chunk_mut().patch_jump(jump);
        }
        Ok(())
    }

    fn compile_if(&mut self, branches: &ast::Branches) -> CompileResult<()> {
        let mut end_jumps = Vec::with_capacity(branches.len());
        let mut has_else = false;

        for (cond, body) in branches {
            let else_jump = if let Some(cond) = cond {
                self.compile_expr(cond)?;
                Some(self.emit(OpCode::JumpIfFalse(0)))
            } else {
                has_else = true;
                None
            };

            self.compile_expr(body)?;
            if cond.is_some() {
                end_jumps.push(self.emit(OpCode::Jump(0)));
            }
            if let Some(else_jump) = else_jump {
                self.chunk_mut().patch_jump(else_jump);
            }
            if cond.is_none() {
                break;
            }
        }

        if !has_else {
            self.emit(OpCode::PushNone);
        }

        for jump in end_jumps {
            self.chunk_mut().patch_jump(jump);
        }
        Ok(())
    }

    fn compile_unless(&mut self, branches: &ast::Branches) -> CompileResult<()> {
        let Some((condition, body)) = branches.first() else {
            self.emit(OpCode::PushNone);
            return Ok(());
        };
        if let Some(condition) = condition {
            self.compile_expr(condition)?;
            self.emit(OpCode::Not);
            let skip = self.emit(OpCode::JumpIfFalse(0));
            self.compile_expr(body)?;
            let end = self.emit(OpCode::Jump(0));
            self.chunk_mut().patch_jump(skip);
            self.emit(OpCode::PushNone);
            self.chunk_mut().patch_jump(end);
        } else {
            self.compile_expr(body)?;
        }
        Ok(())
    }

    fn compile_loop_body(
        &mut self,
        continue_target: usize,
        acc_slot: u16,
        body: &Program,
    ) -> CompileResult<Vec<usize>> {
        self.loops.push(LoopCtx {
            continue_target: ContinueTarget::Fixed(continue_target),
            break_jumps: Vec::new(),
            acc_slot,
            chunk_index: self.current,
        });
        self.emit(OpCode::GetLocal(acc_slot));
        self.emit(OpCode::SetLocal(SELF_SLOT));
        self.compile_body(body)?;
        self.emit(OpCode::SetLocal(acc_slot));
        let back = self.chunk_mut().backward_offset(continue_target);
        self.emit(OpCode::Jump(back));
        Ok(self.loops.pop().unwrap().break_jumps)
    }

    fn compile_foreach(&mut self, ident: crate::Ident, iterable: &Shared<Node>, body: &Program) -> CompileResult<()> {
        let acc_slot = self.scope_mut().declare_synthetic();
        self.emit(OpCode::ArrayNew);
        self.emit(OpCode::SetLocal(acc_slot));

        let array_slot = self.scope_mut().declare_synthetic();
        self.compile_expr(iterable)?;
        self.emit(OpCode::ToForeachIterable);
        self.emit(OpCode::SetLocal(array_slot));

        let index_slot = self.scope_mut().declare_synthetic();
        let zero = self.chunk_mut().push_const(RuntimeValue::Number(0.0.into()));
        self.emit(OpCode::Const(zero));
        self.emit(OpCode::SetLocal(index_slot));

        let loop_var_slot = self.scope_mut().declare(ident);

        let cond_check = self.chunk_mut().code.len();
        self.emit(OpCode::GetLocal(index_slot));
        self.emit(OpCode::GetLocal(array_slot));
        self.emit(OpCode::ArrayLen);
        self.emit(OpCode::Lt);
        let exit_jump = self.emit(OpCode::JumpIfFalse(0));

        self.emit(OpCode::GetLocal(array_slot));
        self.emit(OpCode::GetLocal(index_slot));
        self.emit(OpCode::ArrayGetAt);
        self.emit(OpCode::SetLocal(loop_var_slot));
        self.emit(OpCode::GetLocal(loop_var_slot));
        self.emit(OpCode::SetLocal(SELF_SLOT));

        self.loops.push(LoopCtx {
            continue_target: ContinueTarget::Pending(Vec::new()),
            break_jumps: Vec::new(),
            acc_slot,
            chunk_index: self.current,
        });

        self.compile_body(body)?;
        self.emit(OpCode::GetLocal(acc_slot));
        self.emit(OpCode::Swap);
        self.emit(OpCode::ArrayPush);
        self.emit(OpCode::SetLocal(acc_slot));

        let finished = self.loops.pop().unwrap();
        let ContinueTarget::Pending(continue_sites) = finished.continue_target else {
            unreachable!("foreach always pushes a Pending continue target")
        };
        for site in continue_sites {
            self.chunk_mut().patch_jump(site);
        }
        self.emit(OpCode::GetLocal(index_slot));
        let one = self.chunk_mut().push_const(RuntimeValue::Number(1.0.into()));
        self.emit(OpCode::Const(one));
        self.emit(OpCode::Add);
        self.emit(OpCode::SetLocal(index_slot));
        let back = self.chunk_mut().backward_offset(cond_check);
        self.emit(OpCode::Jump(back));

        self.chunk_mut().patch_jump(exit_jump);
        for jump in finished.break_jumps {
            self.chunk_mut().patch_jump(jump);
        }
        self.emit(OpCode::GetLocal(acc_slot));
        Ok(())
    }

    fn compile_while(&mut self, cond: &Shared<Node>, body: &Program) -> CompileResult<()> {
        self.compile_conditional_loop(cond, body, false)
    }

    fn compile_until(&mut self, cond: &Shared<Node>, body: &Program) -> CompileResult<()> {
        self.compile_conditional_loop(cond, body, true)
    }

    fn compile_conditional_loop(&mut self, cond: &Shared<Node>, body: &Program, invert: bool) -> CompileResult<()> {
        let acc_slot = self.scope_mut().declare_synthetic();
        self.emit(OpCode::PushNone);
        self.emit(OpCode::SetLocal(acc_slot));

        let loop_start = self.chunk_mut().code.len();
        self.compile_expr(cond)?;
        if invert {
            self.emit(OpCode::Not);
        }
        let exit_jump = self.emit(OpCode::JumpIfFalse(0));
        let mut patch_sites = self.compile_loop_body(loop_start, acc_slot, body)?;
        patch_sites.push(exit_jump);

        for jump in patch_sites {
            self.chunk_mut().patch_jump(jump);
        }
        self.emit(OpCode::GetLocal(acc_slot));
        Ok(())
    }

    fn compile_loop(&mut self, body: &Program) -> CompileResult<()> {
        let acc_slot = self.scope_mut().declare_synthetic();
        self.emit(OpCode::PushNone);
        self.emit(OpCode::SetLocal(acc_slot));

        let loop_start = self.chunk_mut().code.len();
        let patch_sites = self.compile_loop_body(loop_start, acc_slot, body)?;

        for jump in patch_sites {
            self.chunk_mut().patch_jump(jump);
        }
        self.emit(OpCode::GetLocal(acc_slot));
        Ok(())
    }
}

fn fast_path_binop(name: &crate::Ident) -> Option<OpCode> {
    let name = *name;
    if name == builtins::ADD.into() {
        Some(OpCode::Add)
    } else if name == builtins::SUB.into() {
        Some(OpCode::Sub)
    } else if name == builtins::MUL.into() {
        Some(OpCode::Mul)
    } else if name == builtins::DIV.into() {
        Some(OpCode::Div)
    } else if name == builtins::MOD.into() {
        Some(OpCode::Mod)
    } else if name == builtins::EQ.into() {
        Some(OpCode::Eq)
    } else if name == builtins::NE.into() {
        Some(OpCode::Ne)
    } else if name == builtins::LT.into() {
        Some(OpCode::Lt)
    } else if name == builtins::LTE.into() {
        Some(OpCode::Le)
    } else if name == builtins::GT.into() {
        Some(OpCode::Gt)
    } else if name == builtins::GTE.into() {
        Some(OpCode::Ge)
    } else {
        None
    }
}

fn literal_to_runtime_value(lit: &Literal) -> RuntimeValue {
    match lit {
        Literal::String(s) => RuntimeValue::String(s.clone()),
        Literal::Bytes(b) => RuntimeValue::Bytes(b.clone()),
        Literal::Number(n) => RuntimeValue::Number(*n),
        Literal::Symbol(i) => RuntimeValue::Symbol(*i),
        Literal::Bool(b) => RuntimeValue::Boolean(*b),
        Literal::None => RuntimeValue::None,
    }
}
