#[cfg(feature = "debugger")]
use std::borrow::Cow;
use std::path::PathBuf;

#[cfg(feature = "debugger")]
use crate::Source;
use crate::io::{Io, NativeIo, SandboxedIo};
#[cfg(feature = "debugger")]
use crate::module::ModuleId;
use crate::tarn;
use crate::{
    ArenaId, ModuleResolver, MqResult, Range, RuntimeValue, Shared, SharedCell, TokenKind,
    module::resolver::DefaultModuleResolver, token_alloc,
};
#[cfg(feature = "debugger")]
use crate::{Debugger, DebuggerHandler};
use crate::{
    ModuleLoader, Token,
    arena::Arena,
    error::{self},
    optimizer::{OptimizationLevel, Optimizer},
    parse,
    runtime::builtin::io_context,
};

/// A compiled mq program bundled with its original source, returned by [`Engine::compile`].
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    pub(crate) source: String,
    pub(crate) program: crate::ast::Program,
    #[cfg(not(feature = "debugger"))]
    vm_cache: Option<Shared<SharedCell<Option<Shared<tarn::CachedProgram>>>>>,
}

impl CompiledProgram {
    /// Returns the original source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the underlying AST nodes.
    pub fn program(&self) -> &crate::ast::Program {
        &self.program
    }

    #[cfg(not(feature = "debugger"))]
    pub(crate) fn cached_vm_program(&self) -> Option<Option<Shared<tarn::CachedProgram>>> {
        let cache = self.vm_cache.as_ref()?;
        #[cfg(feature = "sync")]
        {
            Some(cache.read().unwrap().clone())
        }
        #[cfg(not(feature = "sync"))]
        {
            Some(cache.borrow().clone())
        }
    }

    #[cfg(not(feature = "debugger"))]
    pub(crate) fn cache_vm_program(&self, program: Shared<tarn::CachedProgram>) {
        let Some(cache) = &self.vm_cache else {
            return;
        };
        #[cfg(feature = "sync")]
        {
            *cache.write().unwrap() = Some(program);
        }
        #[cfg(not(feature = "sync"))]
        {
            *cache.borrow_mut() = Some(program);
        }
    }
}

impl From<crate::ast::Program> for CompiledProgram {
    /// Wraps a raw `Program` (e.g. from `ast_from_json`) with no source context.
    fn from(program: crate::ast::Program) -> Self {
        Self {
            source: String::new(),
            program,
            #[cfg(not(feature = "debugger"))]
            vm_cache: Some(Shared::new(SharedCell::new(None))),
        }
    }
}

/// The main execution engine for the mq.
///
/// The `Engine` manages parsing, optimization, and evaluation of mq code.
/// It provides methods for configuration, loading modules, and evaluating code.
///
/// # Examples
///
/// ```rust
/// use mq_lang::DefaultEngine;
///
/// let mut engine = DefaultEngine::default();
/// engine.load_builtin_module();
///
/// let input = mq_lang::parse_text_input("hello").unwrap();
/// let result = engine.eval("add(\" world\")", input.into_iter());
/// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
/// ```
#[derive(Debug, Clone)]
pub struct Engine<T: ModuleResolver = DefaultModuleResolver, IO: Io = SandboxedIo<NativeIo>> {
    /// VM state — see [`tarn::VmState`].
    pub(crate) vm: tarn::VmState<T, IO>,
    token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
    optimization_level: OptimizationLevel,
    vm_module_prelude: Vec<VmModulePrelude>,
}

/// A module explicitly prepared through the Engine API, replayed before VM compilation.
/// The VM needs their AST declarations present while it statically resolves the user's query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VmModulePrelude {
    Include(String),
    Import(String),
}

fn create_default_token_arena() -> Shared<SharedCell<Arena<Shared<Token>>>> {
    let token_arena = Shared::new(SharedCell::new(Arena::new(2048)));
    token_alloc(
        &token_arena,
        &Shared::new(Token {
            // Ensure at least one token for ArenaId::new(0)
            kind: TokenKind::Eof, // Dummy token
            range: Range::default(),
            module_id: ArenaId::new(0), // Dummy module_id
        }),
    );
    token_arena
}

impl<T: ModuleResolver> Default for Engine<T, SandboxedIo<NativeIo>> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

// `new` is pinned to the default `IO` (rather than generic over any `IO: Default`) because Rust
// doesn't use a generic parameter's default as an inference fallback — callers like
// `Engine::new(resolver)` have no other way to pin `IO`. Mirrors the `HashMap::new`/
// `HashMap::with_hasher` split. To use a different `IO`, annotate the binding's type, e.g.
// `let engine: Engine<T, MyIo> = Engine::new(resolver);` (this only fixes the *value* passed at
// construction; use `set_io` afterwards to install it).
impl<T: ModuleResolver> Engine<T, SandboxedIo<NativeIo>> {
    pub fn new(module_resolver: T) -> Self {
        let token_arena = create_default_token_arena();
        let module_loader = ModuleLoader::new(module_resolver);
        Self {
            vm: tarn::VmState::with_module_loader(module_loader),
            token_arena,
            optimization_level: OptimizationLevel::default(),
            vm_module_prelude: Vec::new(),
        }
    }

    /// Evaluates `code` against a paused Tarn VM frame's bindings, with `input` bound to
    /// `.`/`self`.
    #[cfg(feature = "debugger")]
    pub fn eval_debug_expression(
        &mut self,
        code: &str,
        input: RuntimeValue,
        bindings: &[(crate::Ident, RuntimeValue)],
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
    /// Like the [`SandboxedIo<NativeIo>`]-pinned [`Engine::new`], but generic over `IO` and
    /// takes the [`Io`] value up front — for hosts that need to select the `Io` *type* at
    /// construction time, not just its value via [`set_io`](Self::set_io). Useful for e.g. a
    /// test runner that wants an in-memory mock `Io` installed from the start.
    ///
    /// This only wires the evaluator side (builtins); pass the same `io` to
    /// [`DefaultModuleResolver::with_io`] when constructing the resolver so local-filesystem
    /// module resolution is gated consistently.
    pub fn with_io(module_resolver: T, io: Shared<IO>) -> Self {
        let token_arena = create_default_token_arena();
        let module_loader = ModuleLoader::new(module_resolver);
        Self {
            vm: tarn::VmState::with_module_loader_and_io(module_loader, io),
            token_arena,
            optimization_level: OptimizationLevel::default(),
            vm_module_prelude: Vec::new(),
        }
    }

    /// Set the optimization level for AST transformations applied before evaluation.
    pub fn set_optimization_level(&mut self, level: OptimizationLevel) {
        self.optimization_level = level;
    }

    /// Set the maximum call stack depth for function calls.
    ///
    /// This prevents infinite recursion by limiting how deep function
    /// calls can be nested. Useful for controlling resource usage.
    pub fn set_max_call_stack_depth(&mut self, max_call_stack_depth: u32) {
        self.vm.options.max_call_stack_depth = max_call_stack_depth;
    }

    /// Set the maximum wall-clock duration allowed for a single `eval` call.
    ///
    /// Disabled by default (no timeout). When exceeded, evaluation stops with
    /// `RuntimeError::Timeout`; the deadline is checked periodically inside loops and
    /// function calls, so it may be exceeded slightly before evaluation actually stops.
    pub fn set_timeout(&mut self, timeout: std::time::Duration) {
        self.vm.options.timeout = Some(timeout);
    }

    /// Makes top-level `let`/`var`/`def` bindings from one `eval()` call visible to the next
    /// (e.g. a REPL). Opt-in since it costs a capture/reseed pass per call.
    pub fn enable_query_session(&mut self) {
        self.vm.session = Some(Shared::new(SharedCell::new(Vec::new())));
    }

    /// Sets the [`Io`] this engine uses for file, environment-variable, and network
    /// access — both for builtins (`read_file`, `write_file`, `http`, ...) and for
    /// local module resolution (`include`/`import`). Defaults to an all-denied
    /// [`SandboxedIo`](crate::SandboxedIo) wrapping [`NativeIo`](crate::NativeIo),
    /// so a host must opt in explicitly.
    ///
    /// This only affects the evaluator side (builtins); pass the same `io` to
    /// [`DefaultModuleResolver::with_io`] when constructing the resolver so
    /// local-filesystem module resolution is gated consistently.
    pub fn set_io(&mut self, io: Shared<IO>) {
        self.vm.io = Shared::clone(&io);
    }

    /// Set search paths for module loading.
    ///
    /// These paths will be searched when loading external modules
    /// via the `include` statement in mq code.
    pub fn set_search_paths(&mut self, paths: Vec<PathBuf>) {
        self.vm.module_loader.set_search_paths(paths);
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
    }

    /// Define a string variable that can be used in mq code.
    ///
    /// This allows you to inject values from the host environment
    /// into the mq execution context.
    pub fn define_string_value(&self, name: &str, value: &str) {
        self.define_value(name, RuntimeValue::String(Shared::new(value.to_string())));
    }

    /// Defines an arbitrary runtime value in the current environment.
    pub fn define_value(&self, name: &str, value: RuntimeValue) {
        self.vm.define(crate::Ident::new(name), value);
    }

    /// Registers a native Rust function under `name`, callable from mq code as `name(...)`.
    ///
    /// Accepts two forms:
    ///
    /// - A raw form taking the already-evaluated call arguments as a slice and returning a
    ///   single [`RuntimeValue`]: `|args: &[RuntimeValue]| -> HostFnResult { .. }`.
    /// - A typed form taking up to eight plain Rust arguments (any combination of `i64`, `f64`,
    ///   `String`, `bool`, `RuntimeValue`, `Vec<T>`, or `Option<T>` for a [`ValueAdapter`] `T`),
    ///   returning `Result<R, HostFunctionError>` for an `R: ValueAdapter`. Argument and return
    ///   values are converted to/from `RuntimeValue` automatically; a wrong argument count or
    ///   type is reported as a [`HostFunctionError`] rather than panicking.
    ///
    /// Errors and panics raised by the function are caught at the call boundary and surfaced as
    /// a normal mq runtime error rather than aborting evaluation or unwinding through the
    /// evaluator; recursion depth and the configured timeout ([`Self::set_timeout`]) are
    /// enforced around each call exactly as for a user-defined function call.
    ///
    /// A registered function only fills in a name that would otherwise be undefined: a `def` of
    /// the same name always takes precedence, and so does *any* built-in function name, whether
    /// or not [`Self::load_builtin_module`] was called — name resolution itself falls back to
    /// the built-in table before host functions are ever consulted. Host functions therefore
    /// extend the set of callable names rather than override existing ones; pick a name that
    /// doesn't collide with a builtin.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mq_lang::{DefaultEngine, HostFunctionError, RuntimeValue, Shared};
    ///
    /// let mut engine = DefaultEngine::default();
    /// engine.load_builtin_module();
    ///
    /// // Raw form: works with any number of arguments, matched by hand.
    /// engine.register_fn("shout", |args: &[RuntimeValue]| match args {
    ///     [RuntimeValue::String(s)] => Ok(RuntimeValue::String(Shared::new(format!("{}!", s.to_uppercase())))),
    ///     _ => Err(HostFunctionError::new("shout() expects one string argument")),
    /// });
    ///
    /// // Typed form: argument and return conversions are handled for you.
    /// engine.register_fn("double", |n: i64| Ok(n * 2));
    ///
    /// let input = mq_lang::parse_text_input("hello").unwrap();
    /// let result = engine.eval(r#"shout("hi") + to_string(double(21))"#, input.into_iter());
    /// assert_eq!(result.unwrap(), vec!["HI!42".to_string().into()].into());
    /// ```
    pub fn register_fn<F, Marker>(&self, name: impl Into<crate::Ident>, f: F)
    where
        F: crate::runtime::host::IntoHostFunction<Marker>,
    {
        let name = name.into();
        let f = f.into_host_fn();
        #[cfg(not(feature = "sync"))]
        self.vm.host_functions.borrow_mut().insert_shared(name, f);
        #[cfg(feature = "sync")]
        self.vm.host_functions.write().unwrap().insert_shared(name, f);
    }

    /// Load the built-in function modules.
    ///
    /// This must be called to enable access to standard functions
    /// like `add`, `sub`, `map`, `filter`, etc.
    pub fn load_builtin_module(&mut self) {
        self.vm.load_builtin_module(Shared::clone(&self.token_arena));
    }

    /// Import an external module by name.
    ///
    /// The module will be searched for in the configured search paths
    /// and made available for use in mq code.
    pub fn import_module(&mut self, module_name: &str) -> Result<(), Box<error::Error>> {
        self.vm
            .module_loader
            .load_from_file(module_name, Shared::clone(&self.token_arena))
            .map_err(|e| Box::new(error::Error::from_error("", e.into(), self.vm.module_loader.clone())))?;
        self.vm_module_prelude
            .push(VmModulePrelude::Import(module_name.to_string()));
        Ok(())
    }

    /// Load an external module by name.
    ///
    /// The module will be searched for in the configured search paths
    /// and made available for use in mq code.
    pub fn load_module(&mut self, module_name: &str) -> Result<(), Box<error::Error>> {
        self.vm
            .module_loader
            .load_from_file(module_name, Shared::clone(&self.token_arena))
            .map_err(|e| Box::new(error::Error::from_error("", e.into(), self.vm.module_loader.clone())))?;
        self.vm_module_prelude
            .push(VmModulePrelude::Include(module_name.to_string()));
        Ok(())
    }

    /// The main engine for evaluating mq code.
    ///
    /// The `Engine` manages parsing, optimization, and evaluation of mq.
    /// It provides methods for configuration, loading modules, and evaluating code.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut engine = mq_lang::DefaultEngine::default();
    /// engine.load_builtin_module();
    ///
    /// let input = mq_lang::parse_text_input("hello").unwrap();
    /// let result = engine.eval("add(\" world\")", input.into_iter());
    /// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
    /// ```
    ///
    pub fn eval<I: Iterator<Item = RuntimeValue>>(&mut self, code: &str, input: I) -> MqResult {
        if code.is_empty() {
            return Ok(vec![].into());
        }

        // Scoped before `parse`, not just `eval_compiled_vm`, so bare `$VAR` resolution sees this engine's `Io`.
        let _io_guard = io_context::scoped(Shared::clone(&self.vm.io) as Shared<dyn Io>);
        let program = parse(code, Shared::clone(&self.token_arena))?;
        let program = Optimizer::with_level(self.optimization_level).optimize(program);

        #[cfg(feature = "debugger")]
        self.vm.module_loader.set_source_code(code.to_string());

        let compiled = CompiledProgram {
            source: code.to_string(),
            program,
            #[cfg(not(feature = "debugger"))]
            vm_cache: None,
        };
        self.eval_compiled_vm(&compiled, input.into_iter())
    }

    /// Compiles mq code into a [`CompiledProgram`] that can be evaluated multiple times.
    ///
    /// Use this with `eval_compiled` to avoid re-parsing the same query for each input.
    pub fn compile(&mut self, code: &str) -> Result<CompiledProgram, Box<error::Error>> {
        if code.is_empty() {
            return Ok(CompiledProgram {
                source: String::new(),
                program: vec![],
                #[cfg(not(feature = "debugger"))]
                vm_cache: Some(Shared::new(SharedCell::new(None))),
            });
        }
        let _io_guard = io_context::scoped(Shared::clone(&self.vm.io) as Shared<dyn Io>);
        let program = parse(code, Shared::clone(&self.token_arena))?;
        let program = Optimizer::with_level(self.optimization_level).optimize(program);
        Ok(CompiledProgram {
            source: code.to_string(),
            program,
            #[cfg(not(feature = "debugger"))]
            vm_cache: Some(Shared::new(SharedCell::new(None))),
        })
    }

    /// Evaluates a pre-compiled program against the given input.
    ///
    /// Use with `compile` to avoid re-parsing the same query for each input file,
    /// or with a [`CompiledProgram`] constructed from a deserialized JSON AST (`ast-json` feature).
    ///
    /// # Examples
    ///
    /// ```rust
    /// let mut engine = mq_lang::DefaultEngine::default();
    /// engine.load_builtin_module();
    ///
    /// let compiled = engine.compile("add(\" world\")").unwrap();
    /// let input = mq_lang::parse_text_input("hello").unwrap();
    /// let result = engine.eval_compiled(&compiled, input.into_iter());
    /// assert_eq!(result.unwrap(), vec!["hello world".to_string().into()].into());
    /// ```
    pub fn eval_compiled<I: Iterator<Item = RuntimeValue>>(
        &mut self,
        compiled: &CompiledProgram,
        input: I,
    ) -> MqResult {
        #[cfg(feature = "debugger")]
        self.vm.module_loader.set_source_code(compiled.source.clone());

        self.eval_compiled_vm(compiled, input)
    }

    /// Renders the Tarn bytecode that would be executed for `compiled`.
    ///
    /// This is available only with the `debug-trace` feature and is intended for diagnostic
    /// tools such as `mq-dbg`; it does not execute the program.
    #[cfg(feature = "debug-trace")]
    pub fn dump_bytecode(&mut self, compiled: &CompiledProgram) -> Result<String, Box<error::Error>> {
        self.vm.module_loader.set_source_code(compiled.source.clone());
        let global_bindings = self.vm.global_bindings_snapshot();
        let vm_program = tarn::build_program(
            &compiled.program,
            Shared::clone(&self.token_arena),
            &self.vm_module_prelude,
        )?;
        let vm_program = vm_program.as_ref().unwrap_or(&compiled.program);

        tarn::dump_bytecode(
            vm_program,
            Shared::clone(&self.token_arena),
            self.vm.module_loader.with_same_resolver(),
            &global_bindings,
        )
        .map_err(|error| {
            Box::new(error::Error::from_error(
                &compiled.source,
                error.into_inner_error(Shared::clone(&self.token_arena)),
                self.vm.module_loader.clone(),
            ))
        })
    }

    /// Evaluates one input through the bytecode VM. Same `MqResult` shape as `eval_compiled`.
    pub(crate) fn eval_compiled_vm<I>(&mut self, compiled: &CompiledProgram, input: I) -> MqResult
    where
        I: Iterator<Item = RuntimeValue>,
    {
        // Scoped like `eval`/`eval_compiled`, so bare `$VAR` resolution (and anything else
        // reading the ambient `Io`) inside VM-executed builtins sees this engine's `Io`
        // rather than whatever the previous scope (or none) left in place.
        let _io_guard = io_context::scoped(Shared::clone(&self.vm.io) as Shared<dyn Io>);

        #[cfg(feature = "sync")]
        let host_functions = self.vm.host_functions.read().unwrap().clone();
        #[cfg(not(feature = "sync"))]
        let host_functions = self.vm.host_functions.borrow().clone();

        #[cfg(not(feature = "debugger"))]
        let (global_bindings, environment_key) = self.vm.global_bindings_snapshot_with_key();
        #[cfg(feature = "debugger")]
        let global_bindings = self.vm.global_bindings_snapshot();

        #[cfg(feature = "debugger")]
        let vm_program = tarn::build_program(
            &compiled.program,
            Shared::clone(&self.token_arena),
            &self.vm_module_prelude,
        )?;
        #[cfg(feature = "debugger")]
        let vm_program = vm_program.as_ref().unwrap_or(&compiled.program);

        let (timeout, max_call_stack_depth) = (self.vm.options.timeout, self.vm.options.max_call_stack_depth);
        let module_loader = self.vm.module_loader.with_same_resolver();

        let vm = tarn::TarnVm {
            engine: tarn::EngineRunContext {
                host_functions: &host_functions,
                timeout,
                max_call_stack_depth,
                token_arena: Shared::clone(&self.token_arena),
                module_loader,
                global_bindings: &global_bindings,
                session: self.vm.session.as_ref(),
                preresolved_module_vars: Default::default(),
            },
            #[cfg(not(feature = "debugger"))]
            module_prelude: &self.vm_module_prelude,
            #[cfg(not(feature = "debugger"))]
            environment_key,
            #[cfg(not(feature = "debugger"))]
            module_cache_key: self.vm.module_cache_key,
            #[cfg(feature = "debugger")]
            debugger: Shared::clone(&self.vm.debugger),
            #[cfg(feature = "debugger")]
            debugger_handler: Shared::clone(&self.vm.debugger_handler),
            #[cfg(feature = "debugger")]
            source: Source {
                name: None,
                code: compiled.source.clone(),
            },
        };
        let module_loader_for_error = self.vm.module_loader.clone();
        vm.run(
            compiled,
            #[cfg(feature = "debugger")]
            vm_program,
            #[cfg(not(feature = "debugger"))]
            &compiled.program,
            input,
        )
        .map(Into::into)
        .map_err(|error| {
            Box::new(error::Error::from_error(
                &compiled.source,
                error.into_inner_error(Shared::clone(&self.token_arena)),
                module_loader_for_error,
            ))
        })
    }

    /// Returns a reference to the debugger instance.
    ///
    /// This allows interactive debugging of mq code execution when the
    /// `debugger` feature is enabled. Use this to inspect or control
    /// the execution state for advanced debugging scenarios.
    #[cfg(feature = "debugger")]
    pub fn debugger(&self) -> Shared<SharedCell<Debugger>> {
        Shared::clone(&self.vm.debugger)
    }

    #[cfg(feature = "debugger")]
    pub fn set_debugger_handler(&mut self, handler: Box<dyn DebuggerHandler>) {
        self.vm.debugger_handler = Shared::new(SharedCell::new(handler));
    }

    #[cfg(feature = "debugger")]
    pub fn token_arena(&self) -> Shared<SharedCell<Arena<Shared<Token>>>> {
        Shared::clone(&self.token_arena)
    }

    #[cfg(feature = "debugger")]
    pub fn get_module_name(&self, module_id: ModuleId) -> Cow<'static, str> {
        self.vm.module_loader.module_name(module_id)
    }

    #[cfg(feature = "debugger")]
    pub fn get_source_code_for_debug(&self, module_id: ModuleId) -> Result<String, Box<error::Error>> {
        let module_loader = &self.vm.module_loader;
        let source_code = module_loader.get_source_code_for_debug(module_id);

        source_code.map_err(|e| Box::new(error::Error::from_error("", e.into(), module_loader.clone())))
    }

    /// Resolves `module_name` to the path its resolver loaded it from.
    pub fn get_module_path(&self, module_name: &str) -> Result<String, Box<error::Error>> {
        let module_loader = &self.vm.module_loader;
        module_loader
            .get_module_path(module_name)
            .map_err(|e| Box::new(error::Error::from_error("", e.into(), module_loader.clone())))
    }

    pub const fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}

#[cfg(feature = "http-import-ureq")]
impl Engine<DefaultModuleResolver> {
    /// Replaces the HTTP resolver's domain allowlist.
    ///
    /// An empty list restricts access to the built-in default domain
    /// (`raw.githubusercontent.com/harehare`) only; it does not open up all URLs.
    pub fn set_http_allowed_domains(&mut self, domains: Vec<String>) {
        self.vm.module_loader.set_http_allowed_domains(domains);
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
    }

    /// Enables or disables HTTP module imports outright, independent of the domain allowlist.
    ///
    /// The `mq` CLI calls this with `false` unless `--allow-http-import` is passed, so
    /// imports are opt-in there; disabled regardless of `--allowed-domain`.
    pub fn set_http_import_enabled(&mut self, enabled: bool) {
        self.vm.module_loader.set_http_import_enabled(enabled);
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
    }

    /// Clears all locally-cached HTTP module files.
    ///
    /// Call this once before processing to force a re-fetch of all cached modules
    /// on the next resolve (e.g. when `--refresh-modules` is passed on the CLI).
    pub fn clear_http_cache(&mut self) -> Result<(), crate::module::error::ModuleError> {
        let result = self.vm.module_loader.clear_http_cache();
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
        result
    }

    /// Clears all HTTP module cache including versioned modules and lock files.
    ///
    /// Use this when `--clear-cache` is passed on the CLI to wipe everything.
    pub fn clear_http_cache_all(&mut self) -> Result<(), crate::module::error::ModuleError> {
        let result = self.vm.module_loader.clear_http_cache_all();
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
        result
    }

    /// Enables or disables the `mq.lock` integrity check for HTTP imports (on by default).
    pub fn set_lockfile_enabled(&mut self, enabled: bool) {
        self.vm.module_loader.set_lockfile_enabled(enabled);
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
    }

    /// When `true`, a URL with no existing `mq.lock` entry is a hard error instead of being
    /// recorded as a new entry (off by default). Mirrors `npm ci` / `cargo build --locked`:
    /// pass `--frozen` on the CLI so trusting a module's content for the first time
    /// only ever happens in a reviewable local run, not silently in CI.
    pub fn set_lockfile_frozen(&mut self, frozen: bool) {
        self.vm.module_loader.set_lockfile_frozen(frozen);
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
    }

    /// Sets the path used for `mq.lock`.
    pub fn set_lockfile_path(&mut self, path: std::path::PathBuf) {
        self.vm.module_loader.set_lockfile_path(path);
        #[cfg(not(feature = "debugger"))]
        self.vm.invalidate_module_cache();
    }
}

#[cfg(test)]
mod tests {
    use super::CompiledProgram;
    use crate::DefaultEngine;
    use crate::RuntimeValue;
    use crate::Shared;
    use crate::error;
    use rstest::rstest;
    use scopeguard::defer;
    use std::io::Write;
    use std::{fs::File, path::PathBuf};

    fn create_file(name: &str, content: &str) -> (PathBuf, PathBuf) {
        let temp_dir = std::env::temp_dir();
        let temp_file_path = temp_dir.join(name);
        let mut file = File::create(&temp_file_path).expect("Failed to create temp file");
        file.write_all(content.as_bytes())
            .expect("Failed to write to temp file");

        (temp_dir, temp_file_path)
    }

    #[test]
    fn test_set_paths() {
        let mut engine = DefaultEngine::default();
        let paths = vec![PathBuf::from("/test/path")];
        engine.set_search_paths(paths.clone());
        assert_eq!(engine.vm.module_loader.search_paths(), paths);
    }

    #[test]
    fn test_set_max_call_stack_depth() {
        let mut engine = DefaultEngine::default();
        let default_depth = engine.vm.options.max_call_stack_depth;
        let new_depth = default_depth + 10;

        engine.set_max_call_stack_depth(new_depth);
        assert_eq!(engine.vm.options.max_call_stack_depth, new_depth);
    }

    #[test]
    fn test_set_timeout() {
        let mut engine = DefaultEngine::default();
        assert_eq!(engine.vm.options.timeout, None);

        let timeout = std::time::Duration::from_secs(1);
        engine.set_timeout(timeout);
        assert_eq!(engine.vm.options.timeout, Some(timeout));
    }

    #[rstest]
    #[case::while_loop("while(true): 1;")]
    #[case::bare_loop("loop: 1;")]
    #[case::foreach_large_array("foreach(x, range(999999)): x;")]
    fn test_timeout_aborts_runaway_query(#[case] query: &str) {
        let mut engine = DefaultEngine::default();
        // A zero timeout guarantees the deadline is already passed by the first
        // periodic check, regardless of how fast the machine running this test is.
        engine.set_timeout(std::time::Duration::ZERO);

        let started = std::time::Instant::now();
        let result = engine.eval(query, vec!["".to_string().into()].into_iter());

        assert!(matches!(
            result.unwrap_err().cause,
            error::InnerError::Runtime(error::runtime::RuntimeError::Timeout(_))
        ));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    /// Yields `2000.0` (loop body never runs, so this input always "succeeds" instantly),
    /// then blocks `delay` before yielding `0.0` (loop runs ~2000 steps).
    struct DelayedSecondInput {
        step: u8,
        delay: std::time::Duration,
    }

    impl Iterator for DelayedSecondInput {
        type Item = RuntimeValue;

        fn next(&mut self) -> Option<RuntimeValue> {
            match self.step {
                0 => {
                    self.step = 1;
                    Some(2000.into())
                }
                1 => {
                    self.step = 2;
                    std::thread::sleep(self.delay);
                    Some(0.into())
                }
                _ => None,
            }
        }
    }

    #[test]
    fn test_timeout_is_shared_across_inputs_not_reset_per_input() {
        let mut engine = DefaultEngine::default();
        engine.set_timeout(std::time::Duration::from_millis(100));

        let result = engine.eval(
            "until(. >= 2000): . + 1;",
            DelayedSecondInput {
                step: 0,
                delay: std::time::Duration::from_millis(500),
            },
        );

        assert!(matches!(
            result.unwrap_err().cause,
            error::InnerError::Runtime(error::runtime::RuntimeError::Timeout(_))
        ));
    }

    #[test]
    fn test_default_value_recursion_is_call_depth_limited() {
        let mut engine = DefaultEngine::default();
        engine.set_max_call_stack_depth(50);

        let result = engine.eval("def f(x = f()): x; | f()", vec!["".to_string().into()].into_iter());

        assert!(matches!(
            result.unwrap_err().cause,
            error::InnerError::Runtime(error::runtime::RuntimeError::RecursionError(_))
        ));
    }

    #[test]
    fn test_query_session_persists_let_across_eval_calls() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("let x = 41", vec!["".to_string().into()].into_iter())
            .unwrap();
        let result = engine.eval("x + 1", vec!["".to_string().into()].into_iter());

        assert_eq!(result.unwrap(), vec![42.into()].into());
    }

    #[test]
    fn test_query_session_persists_var_mutation_across_eval_calls() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("var x = 1", vec!["".to_string().into()].into_iter())
            .unwrap();
        engine.eval("x += 1", vec!["".to_string().into()].into_iter()).unwrap();
        let result = engine.eval("x", vec!["".to_string().into()].into_iter());

        assert_eq!(result.unwrap(), vec![2.into()].into());
    }

    #[test]
    fn test_query_session_var_redeclaration_updates_value() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("var x = 1", vec!["".to_string().into()].into_iter())
            .unwrap();
        engine
            .eval("var x = 2", vec!["".to_string().into()].into_iter())
            .unwrap();
        let result = engine.eval("x", vec!["".to_string().into()].into_iter());

        assert_eq!(result.unwrap(), vec![2.into()].into());
    }

    #[test]
    fn test_query_session_persists_def_across_eval_calls() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("def add_one(x): x + 1;", vec!["".to_string().into()].into_iter())
            .unwrap();
        let result = engine.eval("add_one(5)", vec!["".to_string().into()].into_iter());

        assert_eq!(result.unwrap(), vec![6.into()].into());
    }

    #[test]
    fn test_query_session_bindings_are_available_and_updated_across_nodes() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("let base = 41", vec![RuntimeValue::None].into_iter())
            .unwrap();
        let values = engine
            .eval(
                "nodes | let derived = base + 1 | derived",
                vec![RuntimeValue::None].into_iter(),
            )
            .unwrap();
        assert_eq!(values.values(), &[RuntimeValue::Number(42.into())]);

        let persisted = engine.eval("derived", vec![RuntimeValue::None].into_iter()).unwrap();
        assert_eq!(persisted.values(), &[RuntimeValue::Number(42.into())]);
    }

    #[test]
    fn test_query_session_var_mutation_carries_forward_across_inputs_in_one_eval() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("var x = 0", vec!["".to_string().into()].into_iter())
            .unwrap();
        let result = engine.eval(
            "x += 1 | x",
            vec!["".to_string().into(), "".to_string().into(), "".to_string().into()].into_iter(),
        );

        assert_eq!(result.unwrap(), vec![1.into(), 2.into(), 3.into()].into());

        let persisted = engine.eval("x", vec!["".to_string().into()].into_iter());
        assert_eq!(persisted.unwrap(), vec![3.into()].into());
    }

    #[test]
    fn test_query_session_empty_input_iterator_preserves_bindings() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("let x = 1", vec!["".to_string().into()].into_iter())
            .unwrap();
        let result = engine.eval("x + 1", std::iter::empty());
        assert_eq!(result.unwrap(), Vec::<RuntimeValue>::new().into());

        let persisted = engine.eval("x", vec!["".to_string().into()].into_iter());
        assert_eq!(persisted.unwrap(), vec![1.into()].into());
    }

    #[test]
    fn test_query_session_nodes_over_empty_input_preserves_let_binding() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("let x = 1", vec!["".to_string().into()].into_iter())
            .unwrap();
        // Aggregate runs once regardless of input count; must see seeded x == 1, not None.
        let result = engine.eval("nodes | x + 1", std::iter::empty());
        assert_eq!(result.unwrap(), vec![2.into()].into());

        let persisted = engine.eval("x", vec!["".to_string().into()].into_iter());
        assert_eq!(
            persisted.unwrap(),
            vec![1.into()].into(),
            "a `nodes` query over an empty input iterator must not wipe a previously saved `let`"
        );
    }

    #[test]
    fn test_query_session_nodes_over_empty_input_preserves_var_binding() {
        let mut engine = DefaultEngine::default();
        engine.enable_query_session();

        engine
            .eval("var x = 1", vec!["".to_string().into()].into_iter())
            .unwrap();
        let result = engine.eval("nodes | x + 1", std::iter::empty());
        assert_eq!(result.unwrap(), vec![2.into()].into());

        let persisted = engine.eval("x", vec!["".to_string().into()].into_iter());
        assert_eq!(
            persisted.unwrap(),
            vec![1.into()].into(),
            "a `nodes` query over an empty input iterator must not wipe a previously saved `var`"
        );
    }

    #[test]
    fn test_uncached_nodes_split_preserves_let_immutability() {
        let mut engine = DefaultEngine::default();

        let error = engine
            .eval("let x = 1 | nodes | x = 2 | x", vec![RuntimeValue::None].into_iter())
            .unwrap_err();
        assert!(
            error.to_string().contains("Cannot assign to immutable variable \"x\""),
            "{error}"
        );
    }

    #[test]
    fn test_nodes_split_captures_as_bindings() {
        let mut engine = DefaultEngine::default();
        let values = engine
            .eval(
                ". as x | nodes | x",
                vec![RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())].into_iter(),
            )
            .unwrap();

        assert_eq!(values.values(), &[RuntimeValue::Number(2.into())]);
    }

    #[test]
    fn test_query_session_is_opt_in() {
        let mut engine = DefaultEngine::default();

        engine
            .eval("let x = 1", vec!["".to_string().into()].into_iter())
            .unwrap();
        let result = engine.eval("x", vec!["".to_string().into()].into_iter());

        assert!(result.is_err());
    }

    #[test]
    fn test_no_timeout_by_default() {
        let mut engine = DefaultEngine::default();
        let result = engine.eval("1 + 1", vec!["".to_string().into()].into_iter());
        assert!(result.is_ok());
    }

    #[test]
    fn test_version() {
        let version = DefaultEngine::version();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_load_module() {
        let (temp_dir, temp_file_path) = create_file("test_module.mq", "def func1(): 42;");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let result = engine.load_module("test_module");
        assert!(result.is_ok());
    }

    #[test]
    fn test_error_load_module() {
        let (temp_dir, temp_file_path) = create_file("error.mq", "error");
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let result = engine.load_module("error");
        assert!(result.is_err());
    }

    #[test]
    fn test_eval() {
        let mut engine = DefaultEngine::default();
        let result = engine.eval("add(1, 1)", vec!["".to_string().into()].into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn test_eval_import_as_alias() {
        let (temp_dir, temp_file_path) =
            create_file("greeter_engine_test.mq", r#"def greet(name): "Hello, " + name + "!";"#);
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let result = engine.eval(
            r#"import "greeter_engine_test" as g | g::greet("World")"#,
            vec!["".to_string().into()].into_iter(),
        );
        assert!(result.is_ok(), "{result:?}");
        assert_eq!(
            result.unwrap().into_iter().next(),
            Some(crate::RuntimeValue::String(Shared::new("Hello, World!".to_string())))
        );
    }

    #[rstest]
    #[case("add(1, 1)", "add(1, 1)")]
    #[case(".", ".")]
    #[case("length(.)", "length(.)")]
    fn test_compiled_program_source(#[case] query: &str, #[case] expected: &str) {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        assert_eq!(compiled.source(), expected);
        assert!(!compiled.program().is_empty());
        assert_eq!(compiled.clone().source(), expected);
    }

    #[rstest]
    #[case("")]
    fn test_compile_empty_code(#[case] query: &str) {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        assert_eq!(compiled.source(), "");
        assert!(compiled.program().is_empty());
    }

    #[cfg(feature = "debug-trace")]
    #[test]
    fn test_dump_bytecode_renders_vm_instructions() {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile("1 + 2").unwrap();

        let dump = engine.dump_bytecode(&compiled).unwrap();

        assert!(dump.contains("Tarn VM bytecode"));
        assert!(dump.contains("phase: main"));
        assert!(dump.contains("Const 0"));
        assert!(dump.contains("Add"));
        assert!(dump.contains("Return"));
        assert!(dump.contains("[0] 1"));
    }

    // --- builtin cache tests ---

    /// Two sequential engines calling the same builtin functions must produce identical results,
    /// whether the builtin module was loaded from a fresh parse or replayed from the cache.
    #[rstest]
    #[case("add(1, 2)", vec!["".to_string().into()], vec![3.into()])]
    #[case("not(false)", vec!["".to_string().into()], vec![true.into()])]
    #[case("to_string(42)", vec!["".to_string().into()], vec!["42".to_string().into()])]
    fn test_builtin_cache_sequential_engines_consistent(
        #[case] query: &str,
        #[case] input: Vec<crate::RuntimeValue>,
        #[case] expected: Vec<crate::RuntimeValue>,
    ) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();
        let result1 = engine1.eval(query, input.clone().into_iter()).unwrap();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let result2 = engine2.eval(query, input.into_iter()).unwrap();

        assert_eq!(result1.values(), &expected);
        assert_eq!(result2.values(), &expected);
    }

    /// Compiling and evaluating a builtin function call on a second engine (cache path) must
    /// produce the correct result — verifying that token_ids in the compiled program are valid
    /// when the builtin tokens were injected from cache rather than freshly parsed.
    #[rstest]
    #[case("add(1, 2)", vec!["".to_string().into()], vec![3.into()])]
    #[case("not(false)", vec!["".to_string().into()], vec![true.into()])]
    #[case("len(\"hello\")", vec!["".to_string().into()], vec![5.into()])]
    fn test_builtin_cache_eval_compiled_token_ids_valid(
        #[case] query: &str,
        #[case] input: Vec<crate::RuntimeValue>,
        #[case] expected: Vec<crate::RuntimeValue>,
    ) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled = engine2.compile(query).unwrap();
        let result = engine2.eval_compiled(&compiled, input.into_iter()).unwrap();
        assert_eq!(result.values(), &expected);
    }

    /// Runtime errors on a cache-using engine must carry the correct source_code.
    #[rstest]
    #[case("undefined_fn()", "undefined_fn()")]
    #[case("unknown_call(1, 2)", "unknown_call(1, 2)")]
    fn test_builtin_cache_runtime_error_preserves_source(#[case] query: &str, #[case] expected_source: &str) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled = engine2.compile(query).unwrap();
        let err = engine2
            .eval_compiled(&compiled, crate::null_input().into_iter())
            .unwrap_err();
        assert_eq!(err.source_code.inner(), expected_source);
    }

    /// The error location (token offset + span) must point to the erroring identifier in
    /// source_code.  If cached tokens were injected at shifted positions the offset would
    /// land on the wrong character.
    #[rstest]
    #[case("undefined_fn()", "undefined_fn")]
    #[case("1 | undefined_fn()", "undefined_fn")]
    #[case("add(1) | unknown_fn()", "unknown_fn")]
    fn test_builtin_cache_runtime_error_token_location_correct(#[case] query: &str, #[case] expected_ident: &str) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled = engine2.compile(query).unwrap();
        let err = engine2
            .eval_compiled(&compiled, crate::null_input().into_iter())
            .unwrap_err();

        let offset = err.location.offset();
        let len = err.location.len();
        assert_eq!(
            &err.source_code.inner()[offset..offset + len],
            expected_ident,
            "location must point to the erroring identifier, not a shifted position"
        );
        assert_eq!(offset, query.find(expected_ident).unwrap());
    }

    /// Two sequential engines (one possibly fresh-parse, one cache) must produce identical
    /// error locations — confirming that token_id indices are not shifted by cache replay.
    #[rstest]
    #[case("undefined_fn()")]
    #[case("1 | undefined_fn()")]
    #[case("add(1) | unknown_fn()")]
    fn test_builtin_cache_and_fresh_parse_error_location_identical(#[case] query: &str) {
        let mut engine1 = DefaultEngine::default();
        engine1.load_builtin_module();
        let compiled1 = engine1.compile(query).unwrap();
        let err1 = engine1
            .eval_compiled(&compiled1, crate::null_input().into_iter())
            .unwrap_err();

        let mut engine2 = DefaultEngine::default();
        engine2.load_builtin_module();
        let compiled2 = engine2.compile(query).unwrap();
        let err2 = engine2
            .eval_compiled(&compiled2, crate::null_input().into_iter())
            .unwrap_err();

        assert_eq!(
            err1.location, err2.location,
            "error location must be identical regardless of whether builtin cache was used"
        );
    }

    // --- CompiledProgram unit tests ---

    #[test]
    fn test_compiled_program_from_has_empty_source() {
        let compiled = CompiledProgram::from(vec![]);
        assert_eq!(compiled.source(), "");
        assert!(compiled.program().is_empty());
    }

    #[rstest]
    #[case("add(1, 1)", vec!["".to_string().into()], vec![2.into()])]
    #[case("add(\" world\")", vec!["hello".to_string().into()], vec!["hello world".to_string().into()])]
    #[case("add(\" world\")", vec!["hi".to_string().into()], vec!["hi world".to_string().into()])]
    fn test_eval_compiled(
        #[case] query: &str,
        #[case] input: Vec<crate::RuntimeValue>,
        #[case] expected: Vec<crate::RuntimeValue>,
    ) {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        let compiled = engine.compile(query).unwrap();
        let result = engine.eval_compiled(&compiled, input.into_iter());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().values(), &expected);
    }

    #[rstest]
    #[case("undefined_fn()", "undefined_fn()")]
    #[case("unknown()", "unknown()")]
    fn test_eval_compiled_runtime_error_preserves_source(#[case] query: &str, #[case] expected_source: &str) {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(query).unwrap();
        let err = engine
            .eval_compiled(&compiled, crate::null_input().into_iter())
            .unwrap_err();
        assert_eq!(err.source_code.inner(), expected_source);
    }

    #[rstest]
    #[case("undefined_fn()")]
    #[case("unknown()")]
    fn test_eval_compiled_from_program_has_empty_source_in_error(#[case] query: &str) {
        let mut engine = DefaultEngine::default();
        let original = engine.compile(query).unwrap();
        let no_source = CompiledProgram::from(original.program().clone());
        assert_eq!(no_source.source(), "");
        let err = engine
            .eval_compiled(&no_source, crate::null_input().into_iter())
            .unwrap_err();
        assert_eq!(err.source_code.inner(), "");
    }

    #[test]
    fn test_eval_compiled_with_ast() {
        use crate::{AstExpr, AstLiteral, AstNode, Shared};

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();

        let program = vec![Shared::new(AstNode {
            token_id: crate::arena::ArenaId::new(1),
            expr: AstExpr::Literal(AstLiteral::String("hello".to_string())),
        })];

        let compiled = CompiledProgram::from(program);
        let result = engine.eval_compiled(&compiled, crate::null_input().into_iter());
        assert!(result.is_ok());
        let values = result.unwrap();
        assert_eq!(values.len(), 1);
        assert_eq!(values[0], "hello".to_string().into());
    }

    #[cfg(feature = "sync")]
    #[test]
    fn test_engine_thread_usage_with_sync_feature() {
        use std::sync::{Arc, Mutex};

        use crate::Engine;

        let engine: Arc<Mutex<Engine>> = Arc::new(Mutex::new(Engine::default()));
        let engine_clone = Arc::clone(&engine);

        let handle = std::thread::spawn(move || {
            let mut engine = engine_clone.lock().unwrap();
            let result = engine.eval("2 + 3", vec!["".to_string().into()].into_iter());
            assert!(result.is_ok());
            let values = result.unwrap();
            assert_eq!(values.len(), 1);
            assert_eq!(values[0], 5.into());
        });

        handle.join().expect("Threaded engine usage failed");
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_eval_debug_expression_vm() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        let bindings = [(crate::Ident::new("x"), RuntimeValue::Number(41.into()))];

        assert_eq!(
            engine
                .eval_debug_expression("x + 1", RuntimeValue::NONE, &bindings)
                .unwrap()[0],
            RuntimeValue::Number(42.into())
        );
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_eval_debug_expression_vm_sees_current_value_as_self() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();

        assert_eq!(
            engine
                .eval_debug_expression(". + 1", RuntimeValue::Number(41.into()), &[])
                .unwrap()[0],
            RuntimeValue::Number(42.into())
        );
    }

    #[test]
    fn test_eval_compiled_vm_error_is_a_real_miette_diagnostic() {
        use crate::RuntimeValue;

        // `eval_compiled_vm` used to return a bare `Result<_, String>` — this checks it now
        // produces the same public `error::Error` shape `eval_compiled` does: a real cause
        // (not just a `Display` string) with a non-trivial source span pointing at the
        // failing expression.
        let code = "1 | 1 / 0";
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(code).unwrap();
        let vm_err = engine
            .eval_compiled_vm(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap_err();

        assert!(matches!(
            vm_err.cause,
            error::InnerError::Runtime(error::runtime::RuntimeError::ZeroDivision(_))
        ));
        assert!(
            !vm_err.location.is_empty(),
            "span should cover the failing expression, not be empty"
        );
    }

    #[test]
    fn test_eval_compiled_vm_uses_engine_host_functions() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.register_fn("double", |args: &[RuntimeValue]| {
            let RuntimeValue::Number(value) = &args[0] else {
                return Err("expected number".into());
            };
            Ok(RuntimeValue::Number((value.value() * 2.0).into()))
        });
        let compiled = engine.compile("double(21)").unwrap();

        let values = engine
            .eval_compiled_vm(
                &compiled,
                [RuntimeValue::Number(1.0.into()), RuntimeValue::Number(2.0.into())].into_iter(),
            )
            .unwrap();
        assert_eq!(
            values.values(),
            &vec![RuntimeValue::Number(42.0.into()), RuntimeValue::Number(42.0.into())]
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_eval_compiled_vm_caches_module_free_bytecode() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile("def twice(x): x * 2; | twice(21)").unwrap();
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_none()));

        let first = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(first.values(), &[RuntimeValue::Number(42.into())]);
        #[cfg(not(feature = "debugger"))]
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_some()));

        let second = engine
            .eval_compiled(
                &compiled,
                [RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())].into_iter(),
            )
            .unwrap();
        assert_eq!(
            second.values(),
            &[RuntimeValue::Number(42.into()), RuntimeValue::Number(42.into())]
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_eval_compiled_cache_miss_shares_deadline_between_compile_and_run() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.register_fn("slow_init", |_args: &[RuntimeValue]| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            Ok(RuntimeValue::NONE)
        });
        engine.set_timeout(std::time::Duration::from_millis(500));

        // Cache miss: compiling this resolves `slow_init()` once (~300ms), then `loop: 1;`
        // must run out the *remaining* budget, not a fresh 500ms.
        let compiled = engine
            .compile("module m: let x = slow_init() end | m::x | loop: 1;")
            .unwrap();
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_none()));

        let started = std::time::Instant::now();
        let err = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap_err();
        let elapsed = started.elapsed();

        assert!(matches!(
            err.cause,
            error::InnerError::Runtime(error::runtime::RuntimeError::Timeout(_))
        ));
        assert!(
            elapsed < std::time::Duration::from_millis(750),
            "the compile-phase delay should count against the shared deadline instead of \
             resetting it for the run phase: {elapsed:?}"
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_eval_compiled_vm_cached_bytecode_preserves_markdown_input_handling() {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(".h1").unwrap();
        let input = crate::parse_markdown_input("# Heading\n\nBody").unwrap();

        let first = engine.eval_compiled(&compiled, input.clone().into_iter()).unwrap();
        let second = engine.eval_compiled(&compiled, input.into_iter()).unwrap();

        assert_eq!(second, first);
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_eval_compiled_vm_keeps_external_module_bytecode_frozen() {
        use crate::RuntimeValue;

        let (temp_dir, temp_file_path) = create_file("cached_vm_module_test.mq", r#"def greeting(): "first";"#);
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);
        let compiled = engine
            .compile(r#"include "cached_vm_module_test" | greeting()"#)
            .unwrap();

        let first = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(
            first.values(),
            &[RuntimeValue::String(Shared::new("first".to_string()))]
        );
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_some()));

        std::fs::write(&temp_file_path, r#"def greeting(): "second";"#).unwrap();
        let second = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(
            second.values(),
            &[RuntimeValue::String(Shared::new("first".to_string()))],
            "a cached query keeps the module source it compiled"
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_cached_vm_does_not_share_frozen_modules_between_engines() {
        use crate::RuntimeValue;

        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        std::fs::write(first_dir.path().join("greeting.mq"), r#"def greeting(): "first";"#).unwrap();
        std::fs::write(second_dir.path().join("greeting.mq"), r#"def greeting(): "second";"#).unwrap();

        let mut first_engine = DefaultEngine::default();
        first_engine.set_search_paths(vec![first_dir.path().to_owned()]);
        let compiled = first_engine.compile(r#"include "greeting" | greeting()"#).unwrap();
        assert_eq!(
            first_engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::String(Shared::new("first".to_string()))]
        );

        let mut second_engine = DefaultEngine::default();
        second_engine.set_search_paths(vec![second_dir.path().to_owned()]);
        assert_eq!(
            second_engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::String(Shared::new("second".to_string()))]
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_eval_compiled_vm_caches_engine_loaded_module_bytecode() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_module("csv").unwrap();
        let compiled = engine.compile("csv_parse(true)").unwrap();
        let input = || RuntimeValue::String(Shared::new("name,age\nAda,36\n".to_string()));

        let first = engine.eval_compiled(&compiled, std::iter::once(input())).unwrap();
        assert_eq!(first.values().len(), 1);
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_some()));

        let second = engine.eval_compiled(&compiled, std::iter::once(input())).unwrap();
        assert_eq!(second.values(), first.values());
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_eval_compiled_vm_caches_a_program_with_nodes() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        let compiled = engine.compile(". * 10 | nodes | len()").unwrap();
        let inputs = || {
            [
                RuntimeValue::Number(1.0.into()),
                RuntimeValue::Number(2.0.into()),
                RuntimeValue::Number(3.0.into()),
            ]
            .into_iter()
        };

        let first = engine.eval_compiled(&compiled, inputs()).unwrap();
        assert_eq!(first.values(), &[RuntimeValue::Number(3.0.into())]);
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_some()));

        let second = engine.eval_compiled(&compiled, inputs()).unwrap();
        assert_eq!(second.values(), first.values());
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_eval_compiled_vm_recompiles_after_search_paths_change_on_same_engine() {
        use crate::RuntimeValue;

        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        std::fs::write(first_dir.path().join("greeting.mq"), r#"def greeting(): "first";"#).unwrap();
        std::fs::write(second_dir.path().join("greeting.mq"), r#"def greeting(): "second";"#).unwrap();

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![first_dir.path().to_owned()]);
        let compiled = engine.compile(r#"include "greeting" | greeting()"#).unwrap();
        assert_eq!(
            engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::String(Shared::new("first".to_string()))]
        );

        // Same CompiledProgram, same engine — only search paths changed.
        engine.set_search_paths(vec![second_dir.path().to_owned()]);
        assert_eq!(
            engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::String(Shared::new("second".to_string()))],
            "changing search paths on the same engine must invalidate cached VM bytecode"
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_module_resolution_setters_invalidate_module_cache_key() {
        let mut engine = DefaultEngine::default();

        let key = engine.vm.module_cache_key;
        engine.set_search_paths(vec![]);
        assert_ne!(
            engine.vm.module_cache_key, key,
            "set_search_paths must bump the module cache key"
        );

        #[cfg(feature = "http-import-ureq")]
        {
            let key = engine.vm.module_cache_key;
            engine.set_http_import_enabled(true);
            assert_ne!(
                engine.vm.module_cache_key, key,
                "set_http_import_enabled must bump the module cache key"
            );

            let key = engine.vm.module_cache_key;
            engine.set_http_allowed_domains(vec!["example.invalid".to_string()]);
            assert_ne!(
                engine.vm.module_cache_key, key,
                "set_http_allowed_domains must bump the module cache key"
            );

            let key = engine.vm.module_cache_key;
            engine.set_lockfile_enabled(false);
            assert_ne!(
                engine.vm.module_cache_key, key,
                "set_lockfile_enabled must bump the module cache key"
            );

            let key = engine.vm.module_cache_key;
            engine.set_lockfile_frozen(true);
            assert_ne!(
                engine.vm.module_cache_key, key,
                "set_lockfile_frozen must bump the module cache key"
            );

            let key = engine.vm.module_cache_key;
            engine.set_lockfile_path(std::path::PathBuf::from("custom/mq.lock"));
            assert_ne!(
                engine.vm.module_cache_key, key,
                "set_lockfile_path must bump the module cache key"
            );

            let key = engine.vm.module_cache_key;
            engine.clear_http_cache().unwrap();
            assert_ne!(
                engine.vm.module_cache_key, key,
                "clear_http_cache must bump the module cache key"
            );

            let key = engine.vm.module_cache_key;
            engine.clear_http_cache_all().unwrap();
            assert_ne!(
                engine.vm.module_cache_key, key,
                "clear_http_cache_all must bump the module cache key"
            );
        }
    }

    #[test]
    fn test_module_var_initializer_runs_once_per_eval_across_nodes_split() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, temp_file_path) = create_file(
            "side_effect_counter_module.mq",
            "let counter = bump_counter()\n| def get_counter(): counter;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        engine.load_module("side_effect_counter_module").unwrap();
        let baseline = call_count.load(Ordering::SeqCst);

        let compiled = engine.compile(". | nodes | get_counter()").unwrap();
        engine
            .eval_compiled(
                &compiled,
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst) - baseline,
            1,
            "a module-level initializer with a side effect must run exactly once per eval, \
             not once per input and not again in the nodes aggregate phase"
        );
    }

    #[test]
    fn test_module_var_initializer_runs_once_per_eval_without_nodes() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, temp_file_path) = create_file(
            "side_effect_counter_plain.mq",
            "let counter = bump_counter()\n| def get_counter(): counter;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        engine.load_module("side_effect_counter_plain").unwrap();
        let baseline = call_count.load(Ordering::SeqCst);

        let compiled = engine.compile("get_counter()").unwrap();
        let result = engine
            .eval_compiled(
                &compiled,
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst) - baseline,
            1,
            "the initializer must run once for the whole eval, not once per input row"
        );
        let expected = RuntimeValue::Number((baseline + 1).into());
        assert_eq!(result.values(), &[expected.clone(), expected.clone(), expected]);
    }

    #[test]
    fn test_module_var_initializer_runs_once_per_eval_for_import() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, temp_file_path) = create_file(
            "side_effect_counter_import.mq",
            "let counter = bump_counter()\n| def get_counter(): counter;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        // `import` always addresses its vars through the qualified (aliased) compile path,
        // unlike `include`'s plain named locals — a separate code path this fix must also cover.
        engine.import_module("side_effect_counter_import").unwrap();
        let baseline = call_count.load(Ordering::SeqCst);

        let compiled = engine
            .compile(". | nodes | side_effect_counter_import::get_counter()")
            .unwrap();
        engine
            .eval_compiled(
                &compiled,
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst) - baseline, 1);
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_module_var_initializer_value_is_pinned_across_cached_eval_calls() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, temp_file_path) = create_file(
            "side_effect_counter_cache.mq",
            "let counter = bump_counter()\n| def get_counter(): counter;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        engine.load_module("side_effect_counter_cache").unwrap();
        let baseline = call_count.load(Ordering::SeqCst);
        let compiled = engine.compile("get_counter()").unwrap();

        let first = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_some()));
        let second = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();

        assert_eq!(
            second.values(),
            first.values(),
            "a cache hit must reuse the value baked in on first compile, not recompute it"
        );
        assert_eq!(
            call_count.load(Ordering::SeqCst) - baseline,
            1,
            "the initializer must run only on the compile that populates the cache"
        );
    }

    #[test]
    fn test_module_var_initializer_runs_once_per_eval_with_query_session() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, temp_file_path) = create_file(
            "side_effect_counter_session.mq",
            "let counter = bump_counter()\n| def get_counter(): counter;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);
        engine.enable_query_session();

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        engine.load_module("side_effect_counter_session").unwrap();
        let baseline = call_count.load(Ordering::SeqCst);

        engine
            .eval(
                ". | nodes | get_counter()",
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst) - baseline, 1);
    }

    #[test]
    fn test_module_var_initializer_runs_once_per_eval_for_aliased_import() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, temp_file_path) = create_file(
            "side_effect_counter_aliased_import.mq",
            "let counter = bump_counter()\n| def get_counter(): counter;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        // An explicit `as` alias, unlike `import_module`'s default (module-name) alias.
        let compiled = engine
            .compile(r#"import "side_effect_counter_aliased_import" as counters | . | nodes | counters::get_counter()"#)
            .unwrap();
        engine
            .eval_compiled(
                &compiled,
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_module_var_initializer_error_propagates() {
        let (temp_dir, temp_file_path) = create_file(
            "erroring_var_module.mq",
            "let value = boom()\n| def get_value(): value;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);
        engine.register_fn("boom", |_args: &[RuntimeValue]| {
            Err(crate::HostFunctionError::new("something went wrong"))
        });

        // Reference the module inline (not via `Engine::load_module`, which would eagerly
        // prepare it and fail before Tarn's own resolution runs).
        let compiled = engine
            .compile(r#"include "erroring_var_module" | get_value()"#)
            .unwrap();
        let err = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap_err();

        assert!(err.to_string().contains("something went wrong"), "{err}");
    }

    #[test]
    fn test_inline_module_var_initializer_referencing_enclosing_def_runs_once_per_eval() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let mut engine = DefaultEngine::default();

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        let compiled = engine
            .compile("def helper(): bump_counter(); | module counters: let counter = helper() end | counters::counter")
            .unwrap();
        engine
            .eval_compiled(
                &compiled,
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "a module var initializer referencing an enclosing `def` must still run once per eval"
        );
    }

    #[test]
    fn test_inline_module_var_initializer_runtime_error_is_not_silently_retried() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let mut engine = DefaultEngine::default();

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        // No enclosing-scope dependency: the probe compiles fine and fails at runtime, so a
        // silent probe-failure fallback would additionally retry this during real execution.
        engine.register_fn("boom", move |_args: &[RuntimeValue]| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            Err(crate::HostFunctionError::new("something went wrong"))
        });

        let compiled = engine
            .compile("module counters: let counter = boom() end | counters::counter")
            .unwrap();
        let err = engine
            .eval_compiled(
                &compiled,
                [RuntimeValue::Number(1.0.into()), RuntimeValue::Number(2.0.into())].into_iter(),
            )
            .unwrap_err();

        assert!(err.to_string().contains("something went wrong"), "{err}");
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "a genuine initializer error must be surfaced once, not swallowed and retried during real execution"
        );
    }

    #[test]
    fn test_inline_module_var_initializer_runs_once_per_eval_across_nodes_split() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let mut engine = DefaultEngine::default();

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        let compiled = engine
            .compile("module counters: let counter = bump_counter() end | nodes | counters::counter")
            .unwrap();
        engine
            .eval_compiled(
                &compiled,
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "an inline module's initializer must run exactly once per eval too"
        );
    }

    #[test]
    fn test_module_var_initializer_runs_once_per_eval_when_imported_inside_an_inline_module() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, temp_file_path) = create_file(
            "side_effect_counter_nested.mq",
            "let counter = bump_counter()\n| def get_counter(): counter;\n",
        );
        let temp_file_path_cleanup = temp_file_path.clone();
        defer! {
            if temp_file_path_cleanup.exists() {
                std::fs::remove_file(&temp_file_path_cleanup).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);

        let call_count = Arc::new(AtomicI64::new(0));
        let call_count_clone = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = call_count_clone.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        let compiled = engine
            .compile(r#"module outer: import "side_effect_counter_nested" as m end | nodes | m::get_counter()"#)
            .unwrap();
        engine
            .eval_compiled(
                &compiled,
                [
                    RuntimeValue::Number(1.0.into()),
                    RuntimeValue::Number(2.0.into()),
                    RuntimeValue::Number(3.0.into()),
                ]
                .into_iter(),
            )
            .unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_nested_external_module_var_initializer_runs_once_per_eval() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let (temp_dir, inner_path) = create_file(
            "side_effect_counter_external_inner.mq",
            "let counter = bump_counter()\n| def current_counter(): counter;\n",
        );
        let outer_path = temp_dir.join("side_effect_counter_external_outer.mq");
        std::fs::write(
            &outer_path,
            "include \"side_effect_counter_external_inner\"\n| let outer_counter = bump_counter()\n| def get_counter(): current_counter() + outer_counter;\n",
        )
        .expect("Failed to write outer module");
        defer! {
            for path in [&inner_path, &outer_path] {
                if path.exists() {
                    std::fs::remove_file(path).expect("Failed to delete temp module");
                }
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);
        let call_count = Arc::new(AtomicI64::new(0));
        let counter = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        engine.load_module("side_effect_counter_external_outer").unwrap();
        let baseline = call_count.load(Ordering::SeqCst);
        let compiled = engine.compile("get_counter()").unwrap();
        let result = engine
            .eval_compiled(
                &compiled,
                [RuntimeValue::None, RuntimeValue::None, RuntimeValue::None].into_iter(),
            )
            .unwrap();

        assert_eq!(call_count.load(Ordering::SeqCst) - baseline, 2);
        assert_eq!(result.values().len(), 3);
    }

    #[test]
    fn test_nested_inline_module_var_initializers_run_once_each_per_eval() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicI64, Ordering};

        let mut engine = DefaultEngine::default();
        let call_count = Arc::new(AtomicI64::new(0));
        let counter = Arc::clone(&call_count);
        engine.register_fn("bump_counter", move |_args: &[RuntimeValue]| {
            let value = counter.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(RuntimeValue::Number(value.into()))
        });

        let compiled = engine
            .compile(
                "module outer: let outer_counter = bump_counter() | \
                 module inner: let inner_counter = bump_counter() end end | \
                 nodes | outer::outer_counter",
            )
            .unwrap();
        engine
            .eval_compiled(
                &compiled,
                [RuntimeValue::None, RuntimeValue::None, RuntimeValue::None].into_iter(),
            )
            .unwrap();

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            2,
            "each initializer in a nested inline module tree must run exactly once"
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_cached_nodes_split_preserves_let_immutability() {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile("let x = 1 | nodes | x = 2 | x").unwrap();

        let error = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap_err();
        assert!(
            error.to_string().contains("Cannot assign to immutable variable \"x\""),
            "{error}"
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_cached_nodes_split_keeps_a_shadowing_var_mutable() {
        let mut engine = DefaultEngine::default();
        let compiled = engine.compile("let x = 1 | var x = 2 | nodes | x = 3 | x").unwrap();

        let values = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(values.values(), &[RuntimeValue::Number(3.0.into())]);
    }

    // `eval_compiled_vm` reads `define_value`/`define_string_value` bindings from `self.vm`,
    // which only exists under `tarn`.
    #[test]
    fn test_eval_compiled_vm_resolves_names_defined_via_define_value() {
        use crate::RuntimeValue;

        // Regression test for the gap `mq-ffi`'s `test_define_string_value_and_use_in_eval`/
        // `test_define_string_value_overwrites_previous` caught under `--all-features`
        // (`OpCode::GetExternalGlobal`, seeded from `VmState::global_bindings_snapshot`).
        let mut engine = DefaultEngine::default();
        engine.define_string_value("greeting", "hello");
        engine.define_value("answer", RuntimeValue::Number(42.0.into()));
        let compiled = engine.compile("[greeting, answer]").unwrap();

        let values = engine
            .eval_compiled_vm(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(
            values.values(),
            &vec![RuntimeValue::Array(crate::Shared::new(vec![
                RuntimeValue::String(Shared::new("hello".to_string())),
                RuntimeValue::Number(42.0.into()),
            ]))]
        );

        // Re-defining a value updates an existing cached program. The cache is keyed by
        // global names, rather than values, so compiling once does not make later values stale.
        engine.define_string_value("greeting", "goodbye");
        let values = engine
            .eval_compiled_vm(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(
            values.values(),
            &vec![RuntimeValue::Array(crate::Shared::new(vec![
                RuntimeValue::String(Shared::new("goodbye".to_string())),
                RuntimeValue::Number(42.0.into()),
            ]))]
        );
        #[cfg(not(feature = "debugger"))]
        assert!(compiled.cached_vm_program().is_some_and(|cache| cache.is_some()));
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_cached_vm_reuses_current_globals_for_every_input() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.define_value("offset", RuntimeValue::Number(40.into()));
        let compiled = engine.compile(". + offset").unwrap();

        let values = engine
            .eval_compiled(
                &compiled,
                [RuntimeValue::Number(1.into()), RuntimeValue::Number(2.into())].into_iter(),
            )
            .unwrap();
        assert_eq!(
            values.values(),
            &[RuntimeValue::Number(41.into()), RuntimeValue::Number(42.into())]
        );

        // Values are deliberately not part of the bytecode-cache key. Each batch must instead
        // read a single, current global environment for all of its inputs.
        engine.define_value("offset", RuntimeValue::Number(100.into()));
        let values = engine
            .eval_compiled(&compiled, std::iter::once(RuntimeValue::Number(1.into())))
            .unwrap();
        assert_eq!(values.values(), &[RuntimeValue::Number(101.into())]);
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_cached_vm_keeps_global_environments_separate_between_engines() {
        use crate::RuntimeValue;

        let mut first_engine = DefaultEngine::default();
        first_engine.define_value("offset", RuntimeValue::Number(1.into()));
        let compiled = first_engine.compile("offset").unwrap();
        assert_eq!(
            first_engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::Number(1.into())]
        );

        let mut second_engine = DefaultEngine::default();
        second_engine.define_value("offset", RuntimeValue::Number(2.into()));
        assert_eq!(
            second_engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::Number(2.into())]
        );

        // Switching back must use the first engine's cached environment, not the last engine
        // that happened to evaluate this shared CompiledProgram.
        assert_eq!(
            first_engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::Number(1.into())]
        );
    }

    #[cfg(not(feature = "debugger"))]
    #[test]
    fn test_cached_vm_does_not_reuse_an_environment_after_its_engine_drops() {
        use crate::RuntimeValue;

        let compiled = {
            let mut engine = DefaultEngine::default();
            engine.define_value("offset", RuntimeValue::Number(1.into()));
            let compiled = engine.compile("offset").unwrap();
            assert_eq!(
                engine
                    .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                    .unwrap()
                    .values(),
                &[RuntimeValue::Number(1.into())]
            );
            compiled
        };

        let mut next_engine = DefaultEngine::default();
        next_engine.define_value("offset", RuntimeValue::Number(2.into()));
        assert_eq!(
            next_engine
                .eval_compiled(&compiled, std::iter::once(RuntimeValue::None))
                .unwrap()
                .values(),
            &[RuntimeValue::Number(2.into())]
        );
    }

    #[test]
    fn test_eval_compiled_vm_resolves_a_local_file_import() {
        use crate::RuntimeValue;

        // Mirrors `test_eval_import_as_alias`, but through `eval_compiled_vm` — this only works
        // because `eval_compiled_vm` threads `self.vm.module_loader.clone()` into the VM
        // compiler instead of it hardcoding an in-memory-only `StdModuleResolver`
        // (`STANDARD_MODULES` only, no filesystem access).
        let (temp_dir, temp_file_path) = create_file(
            "greeter_vm_engine_test.mq",
            r#"def greet(name): "Hello, " + name + "!";"#,
        );
        let temp_file_path_clone = temp_file_path.clone();

        defer! {
            if temp_file_path_clone.exists() {
                std::fs::remove_file(&temp_file_path_clone).expect("Failed to delete temp file");
            }
        }

        let mut engine = DefaultEngine::default();
        engine.set_search_paths(vec![temp_dir]);
        let compiled = engine
            .compile(r#"import "greeter_vm_engine_test" as g | g::greet("World")"#)
            .unwrap();

        let values = engine
            .eval_compiled_vm(&compiled, std::iter::once(RuntimeValue::None))
            .unwrap();
        assert_eq!(
            values.values(),
            &vec![RuntimeValue::String(Shared::new("Hello, World!".to_string()))]
        );
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_eval_compiled_vm_notifies_debugger_of_uncaught_error() {
        use crate::{DebugContext, DebuggerHandler, RuntimeValue};
        use std::sync::{Arc, Mutex};

        #[derive(Debug)]
        struct ErrorHandler(Arc<Mutex<Vec<String>>>);

        impl DebuggerHandler for ErrorHandler {
            fn on_error(&self, message: &str, _context: &DebugContext) {
                self.0.lock().unwrap().push(message.to_string());
            }
        }

        let errors = Arc::new(Mutex::new(Vec::new()));
        let mut engine = DefaultEngine::default();
        engine.set_debugger_handler(Box::new(ErrorHandler(Arc::clone(&errors))));
        engine.debugger().write().unwrap().activate();
        let compiled = engine.compile("1 / 0").unwrap();

        assert!(
            engine
                .eval_compiled_vm(&compiled, std::iter::once(RuntimeValue::None))
                .is_err()
        );
        assert!(
            errors
                .lock()
                .unwrap()
                .iter()
                // "Division by zero" — `RuntimeError::ZeroDivision`'s wording, the same the
                // tree-walker uses; `notify_error` converts through it rather than using
                // `VmError`'s own (not user-facing) `Display`. See `notify_error`'s doc comment.
                .any(|message| message.contains("Division by zero"))
        );
    }

    #[cfg(feature = "debugger")]
    #[test]
    fn test_get_source_code_for_debug() {
        use crate::module::ModuleId;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();

        let module_id = ModuleId::new(0);
        let result = engine.get_source_code_for_debug(module_id);

        assert!(result.is_ok());
    }

    #[test]
    fn test_register_fn_basic() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("greet", |args: &[RuntimeValue]| match args {
            [RuntimeValue::String(name)] => Ok(RuntimeValue::String(Shared::new(format!("Hello, {name}!")))),
            _ => Err(crate::HostFunctionError::new("greet() expects one string argument")),
        });

        let result = engine.eval(r#"greet("World")"#, crate::null_input().into_iter());
        assert_eq!(result.unwrap(), vec!["Hello, World!".to_string().into()].into());
    }

    #[test]
    fn test_register_fn_receives_evaluated_args() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("identity", |args: &[RuntimeValue]| {
            Ok(args.first().cloned().unwrap_or(RuntimeValue::NONE))
        });

        let result = engine.eval("identity(1 + 1)", crate::null_input().into_iter());
        assert_eq!(result.unwrap(), vec![2.into()].into());
    }

    #[test]
    fn test_register_fn_works_without_builtin_module_loaded() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.register_fn("triple", |args: &[RuntimeValue]| match args {
            [RuntimeValue::Number(n)] => Ok(RuntimeValue::from(crate::number::Number::from(n.value() * 3.0))),
            _ => Err(crate::HostFunctionError::new("triple() expects one number")),
        });

        let result = engine.eval("triple(2)", crate::null_input().into_iter());
        assert_eq!(
            result.unwrap(),
            vec![RuntimeValue::from(crate::number::Number::from(6_i64))].into()
        );
    }

    #[test]
    fn test_register_fn_does_not_override_builtin_name() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.register_fn("len", |_args: &[RuntimeValue]| {
            Ok(RuntimeValue::from(crate::number::Number::from(-1_i64)))
        });

        let result = engine.eval(r#"len("hello")"#, crate::null_input().into_iter());
        assert_eq!(result.unwrap(), vec![5.into()].into());
    }

    #[test]
    fn test_register_fn_shadowed_by_user_def() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("answer", |_args: &[RuntimeValue]| {
            Ok(RuntimeValue::from(crate::number::Number::from(1)))
        });

        let result = engine.eval("def answer(): 42; | answer()", crate::null_input().into_iter());
        assert_eq!(result.unwrap(), vec![42.into()].into());
    }

    #[test]
    fn test_register_fn_overwrite() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("f", |_args: &[RuntimeValue]| Ok(RuntimeValue::Boolean(false)));
        engine.register_fn("f", |_args: &[RuntimeValue]| Ok(RuntimeValue::Boolean(true)));

        let result = engine.eval("f()", crate::null_input().into_iter());
        assert_eq!(result.unwrap(), vec![true.into()].into());
    }

    #[test]
    fn test_register_fn_error_propagates() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("boom", |_args: &[RuntimeValue]| {
            Err(crate::HostFunctionError::new("something went wrong"))
        });

        let err = engine.eval("boom()", crate::null_input().into_iter()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("boom"), "{message}");
        assert!(message.contains("something went wrong"), "{message}");
        assert!(matches!(
            err.cause,
            error::InnerError::Runtime(error::runtime::RuntimeError::HostFunctionError(_, _, _))
        ));
    }

    #[test]
    fn test_register_fn_panic_is_caught() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("crash", |_args: &[RuntimeValue]| -> crate::HostFnResult {
            panic!("host function bug");
        });

        let err = engine.eval("crash()", crate::null_input().into_iter()).unwrap_err();
        assert!(err.to_string().contains("panic"), "{}", err.to_string());
    }

    #[test]
    fn test_register_fn_respects_timeout() {
        use crate::RuntimeValue;

        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("noop", |_args: &[RuntimeValue]| Ok(RuntimeValue::NONE));
        // A zero timeout guarantees the deadline has already passed by the first
        // periodic check inside the host function call, regardless of machine speed.
        engine.set_timeout(std::time::Duration::ZERO);

        let err = engine
            .eval("while(true): noop(); ", crate::null_input().into_iter())
            .unwrap_err();
        assert!(matches!(
            err.cause,
            error::InnerError::Runtime(error::runtime::RuntimeError::Timeout(_))
        ));
    }

    #[test]
    fn test_register_fn_typed_zero_args() {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("answer", || Ok(42_i64));

        let result = engine.eval("answer()", crate::null_input().into_iter());
        assert_eq!(
            result.unwrap(),
            vec![crate::RuntimeValue::from(crate::number::Number::from(42_i64))].into()
        );
    }

    #[test]
    fn test_register_fn_typed_one_arg() {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("double", |n: i64| Ok(n * 2));

        let result = engine.eval("double(21)", crate::null_input().into_iter());
        assert_eq!(
            result.unwrap(),
            vec![crate::RuntimeValue::from(crate::number::Number::from(42_i64))].into()
        );
    }

    #[test]
    fn test_register_fn_typed_multiple_args_and_types() {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("repeat", |s: String, n: i64| Ok(s.repeat(n as usize)));

        let result = engine.eval(r#"repeat("ab", 3)"#, crate::null_input().into_iter());
        assert_eq!(result.unwrap(), vec!["ababab".to_string().into()].into());
    }

    #[test]
    fn test_register_fn_typed_wrong_type_reports_error() {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("double", |n: i64| Ok(n * 2));

        let err = engine
            .eval(r#"double("not a number")"#, crate::null_input().into_iter())
            .unwrap_err();
        assert!(err.to_string().contains("double"), "{}", err.to_string());
    }

    #[test]
    fn test_register_fn_typed_vec_and_option() {
        let mut engine = DefaultEngine::default();
        engine.load_builtin_module();
        engine.register_fn("sum", |xs: Vec<i64>| Ok(xs.into_iter().sum::<i64>()));
        engine.register_fn("first_or", |xs: Vec<i64>, default: Option<i64>| {
            Ok(xs.into_iter().next().or(default))
        });

        let result = engine.eval("sum([1, 2, 3])", crate::null_input().into_iter());
        assert_eq!(
            result.unwrap(),
            vec![crate::RuntimeValue::from(crate::number::Number::from(6_i64))].into()
        );

        let result = engine.eval("first_or([], 9)", crate::null_input().into_iter());
        assert_eq!(
            result.unwrap(),
            vec![crate::RuntimeValue::from(crate::number::Number::from(9_i64))].into()
        );
    }
}
