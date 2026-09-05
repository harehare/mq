use super::interpreter::{DebugEvent, DebugHook};
use super::{compiler, interpreter};
use crate::runtime::host::HostFunctions;
use crate::{
    Breakpoint, DebugContext, Debugger, DebuggerHandler, Ident, ModuleLoader, ModuleResolver, RuntimeValue, Shared,
    SharedCell, Source, TokenArena, get_token,
};

/// Adapts VM boundary events to the shared debugger API.
pub(crate) struct VmDebuggerHook<R: ModuleResolver> {
    debugger: Shared<SharedCell<Debugger>>,
    handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
    token_arena: TokenArena,
    source: Source,
    sources: Vec<(crate::ModuleId, Source)>,
    module_loader: ModuleLoader<R>,
    host_functions: HostFunctions,
    last_context: Option<DebugContext>,
}

impl<R: ModuleResolver> VmDebuggerHook<R> {
    /// Creates a VM debugger adapter for one compiled source.
    pub(crate) fn new(
        debugger: Shared<SharedCell<Debugger>>,
        handler: Shared<SharedCell<Box<dyn DebuggerHandler>>>,
        token_arena: TokenArena,
        source: Source,
        sources: Vec<(crate::ModuleId, Source)>,
        module_loader: ModuleLoader<R>,
        host_functions: HostFunctions,
    ) -> Self {
        Self {
            debugger,
            handler,
            token_arena,
            source,
            sources,
            module_loader,
            host_functions,
            last_context: None,
        }
    }

    pub(crate) fn notify_error(&self, error: &interpreter::VmError) {
        if self.debugger.read().unwrap().is_active()
            && let Some(context) = &self.last_context
        {
            let message = super::vm_error_to_runtime_error(error, Shared::clone(&self.token_arena)).to_string();
            self.handler.read().unwrap().on_error(&message, context);
        }
    }

    pub(crate) fn set_sources(&mut self, sources: Vec<(crate::ModuleId, Source)>) {
        self.sources = sources;
    }

    fn eval_expression(&self, code: &str, bindings: &[(Ident, RuntimeValue)]) -> Option<RuntimeValue> {
        let program = crate::parse(code, Shared::clone(&self.token_arena)).ok()?;
        let names = bindings.iter().map(|(name, _)| *name).collect::<Vec<_>>();
        let values = bindings.iter().map(|(_, value)| value.clone()).collect::<Vec<_>>();
        let compiled = compiler::compile_debug_expression(
            &program,
            Shared::clone(&self.token_arena),
            self.module_loader.with_same_resolver(),
            &names,
        )
        .ok()?;
        interpreter::run_debug_expression(&compiled, RuntimeValue::None, &values, &self.host_functions).ok()
    }

    fn breakpoint_matches(&self, breakpoint: &crate::Breakpoint, bindings: &[(Ident, RuntimeValue)]) -> bool {
        if let Some(condition) = &breakpoint.condition
            && !self
                .eval_expression(condition, bindings)
                .is_some_and(|value| value.is_truthy())
        {
            return false;
        }

        if let Some(hit_condition) = &breakpoint.hit_condition {
            let count = self.debugger.write().unwrap().record_hit(breakpoint.id);
            let trimmed = hit_condition.trim();
            let code = match trimmed.parse::<usize>() {
                Ok(threshold) => format!("hit_count >= {threshold}"),
                Err(_) => trimmed.to_string(),
            };
            let mut hit_bindings = bindings.to_vec();
            hit_bindings.push((Ident::new("hit_count"), RuntimeValue::Number(count.into())));
            if !self
                .eval_expression(&code, &hit_bindings)
                .is_some_and(|value| value.is_truthy())
            {
                return false;
            }
        }

        true
    }

    fn interpolate_log_message(
        &self,
        message: &str,
        current_value: &RuntimeValue,
        bindings: &[(Ident, RuntimeValue)],
        module_id: crate::ModuleId,
    ) -> Option<String> {
        let segments = crate::lexer::parse_interpolation_segments(message, module_id).ok()?;
        segments
            .iter()
            .try_fold(String::with_capacity(message.len()), |mut output, segment| {
                match segment {
                    crate::lexer::token::StringSegment::Text(text, _) => output.push_str(text),
                    crate::lexer::token::StringSegment::Expr(expr, _) => {
                        let expr = expr.trim();
                        if expr == crate::ast::constants::identifiers::SELF {
                            output.push_str(&current_value.to_string());
                        } else if let Some(name) = expr.strip_prefix('$') {
                            output.push_str(&crate::runtime::builtin::io_context::current().env_var(name).ok()?);
                        } else {
                            output.push_str(&self.eval_expression(expr, bindings)?.to_string());
                        }
                    }
                }
                Some(output)
            })
    }
}

impl<R: ModuleResolver> VmDebuggerHook<R> {
    fn build_context(&mut self, event: DebugEvent) -> (DebugContext, Shared<crate::Token>, Vec<(Ident, RuntimeValue)>) {
        let DebugEvent {
            token_id,
            node,
            current_value,
            bindings,
            vm_frame,
            call_stack,
            #[cfg(feature = "debug-trace")]
            operand_stack,
        } = event;
        let token = get_token(Shared::clone(&self.token_arena), token_id);
        let context = DebugContext {
            current_value,
            current_node: node,
            token: Shared::clone(&token),
            call_stack,
            vm_frame,
            #[cfg(feature = "debug-trace")]
            operand_stack,
            source: self
                .sources
                .iter()
                .find(|(module_id, _)| *module_id == token.module_id)
                .map(|(_, source)| source.clone())
                .unwrap_or_else(|| self.source.clone()),
        };
        self.last_context = Some(context.clone());
        (context, token, bindings)
    }
}

impl<R: ModuleResolver> DebugHook for VmDebuggerHook<R> {
    fn on_boundary(&mut self, event: DebugEvent) {
        if !self.debugger.read().unwrap().is_active() {
            return;
        }

        let (context, token, bindings) = self.build_context(event);

        let breakpoint = self
            .debugger
            .read()
            .unwrap()
            .get_hit_breakpoint(&context, Shared::clone(&token));
        if let Some(breakpoint) = breakpoint {
            if !self.breakpoint_matches(&breakpoint, &bindings) {
                return;
            }
            if let Some(message) = &breakpoint.log_message {
                if let Some(message) =
                    self.interpolate_log_message(message, &context.current_value, &bindings, token.module_id)
                {
                    self.handler
                        .read()
                        .unwrap()
                        .on_log_point(&breakpoint, &message, &context);
                }
                return;
            }
            let action = self.handler.read().unwrap().on_breakpoint_hit(&breakpoint, &context);
            self.debugger.write().unwrap().next(action);
        } else if self.debugger.write().unwrap().should_break(&context) {
            let action = self.handler.read().unwrap().on_step(&context);
            self.debugger.write().unwrap().next(action);
        }
    }

    fn on_explicit_breakpoint(&mut self, event: DebugEvent) {
        if !self.debugger.read().unwrap().is_active() {
            return;
        }

        let (context, token, _bindings) = self.build_context(event);
        let breakpoint = Breakpoint {
            id: 0,
            line: token.range.start.line as usize,
            column: Some(token.range.start.column),
            enabled: true,
            ..Default::default()
        };
        let action = self.handler.read().unwrap().on_breakpoint_hit(&breakpoint, &context);
        self.debugger.write().unwrap().next(action);
    }
}
