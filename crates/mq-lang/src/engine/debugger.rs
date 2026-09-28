//! Engine methods for debuggers such as `mq-dap`.
use super::Engine;
use crate::io::{Io, NativeIo, SandboxedIo};
use crate::module::ModuleId;
use crate::runtime::builtin::io_context;
use crate::{
    Arena, Debugger, DebuggerHandler, Ident, ModuleResolver, MqResult, RuntimeValue, Shared, SharedCell, Token, error,
    parse, tarn,
};
use std::borrow::Cow;

impl<T: ModuleResolver> Engine<T, SandboxedIo<NativeIo>> {
    /// Evaluates `code` against a paused Tarn VM frame's bindings, with `input` bound to
    /// `.`/`self`.
    pub fn eval_debug_expression(
        &mut self,
        code: &str,
        input: RuntimeValue,
        bindings: &[(Ident, RuntimeValue)],
    ) -> MqResult {
        let _io_guard = io_context::scoped(Shared::clone(&self.vm.io) as Shared<dyn Io>);
        let program = parse(code, Shared::clone(&self.token_arena))?;
        #[cfg(feature = "sync")]
        let host_functions = self.vm.host_functions.read().unwrap().clone();
        #[cfg(not(feature = "sync"))]
        let host_functions = self.vm.host_functions.borrow().clone();

        tarn::eval_debug_expression(
            &program,
            Shared::clone(&self.token_arena),
            self.vm.module_loader.with_same_resolver(),
            input,
            bindings,
            &host_functions,
        )
        .map(|value| vec![value].into())
        .map_err(|error| {
            Box::new(error::Error::from_error(
                code,
                error.into_inner_error(Shared::clone(&self.token_arena)),
                self.vm.module_loader.clone(),
            ))
        })
    }
}

impl<T: ModuleResolver, IO: Io> Engine<T, IO> {
    /// Returns a reference to the debugger instance.
    ///
    /// This allows interactive debugging of mq code execution when the
    /// `debugger` feature is enabled. Use this to inspect or control
    /// the execution state for advanced debugging scenarios.
    pub fn debugger(&self) -> Shared<SharedCell<Debugger>> {
        Shared::clone(&self.vm.debugger)
    }

    pub fn set_debugger_handler(&mut self, handler: Box<dyn DebuggerHandler>) {
        self.vm.debugger_handler = Shared::new(SharedCell::new(handler));
    }

    pub fn token_arena(&self) -> Shared<SharedCell<Arena<Shared<Token>>>> {
        Shared::clone(&self.token_arena)
    }

    pub fn get_module_name(&self, module_id: ModuleId) -> Cow<'static, str> {
        self.vm.module_loader.module_name(module_id)
    }

    /// Resolves the module `module_id` to the path its resolver loaded it from.
    pub fn get_module_path_by_id(&self, module_id: ModuleId) -> Result<String, Box<error::Error>> {
        let module_loader = &self.vm.module_loader;
        module_loader
            .module_path(module_id)
            .map_err(|e| Box::new(error::Error::from_error("", e.into(), module_loader.clone())))
    }

    pub fn get_source_code_for_debug(&self, module_id: ModuleId) -> Result<String, Box<error::Error>> {
        let module_loader = &self.vm.module_loader;
        let source_code = module_loader.get_source_code_for_debug(module_id);

        source_code.map_err(|e| Box::new(error::Error::from_error("", e.into(), module_loader.clone())))
    }
}
