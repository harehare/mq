use std::path::PathBuf;

mod compiled_program;
#[cfg(feature = "debugger")]
mod debugger;
mod session;

pub use compiled_program::CompiledProgram;
pub(crate) use compiled_program::ProgramBody;
#[cfg(not(feature = "debugger"))]
pub(crate) use compiled_program::VmCache;
pub use session::Session;

#[cfg(feature = "debugger")]
use crate::Source;
use crate::io::{Io, NativeIo, SandboxedIo};
use crate::tarn::{self, SessionBindings};
use crate::{
    ArenaId, Ident, ModuleResolver, MqResult, Range, RuntimeValue, RuntimeValues, Shared, SharedCell, TokenKind,
    layered_token_arena, module::resolver::DefaultModuleResolver, token_alloc,
};

use crate::{
    ModuleLoader, Token, TokenArena,
    arena::Arena,
    error::{self},
    parse,
    runtime::builtin::io_context,
};

/// An error returned when a value cannot be added to an [`Engine`] environment.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DefineValueError {
    /// The value contains execution state that belongs to a particular VM.
    #[error("cannot define a VM-bound {0}")]
    VmBoundValue(&'static str),
}

/// The main execution engine for the mq.
///
/// The `Engine` manages parsing and evaluation of mq code.
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
    pub(crate) token_arena: Shared<SharedCell<Arena<Shared<Token>>>>,
    pub(crate) vm_module_prelude: Vec<VmModulePrelude>,
    /// Loaded `.mqc` programs, keyed by checksum.
    #[cfg(feature = "mqc")]
    pub(crate) mqc_programs: rustc_hash::FxHashMap<[u8; crate::mqc::CHECKSUM_LEN], CompiledProgram>,
}

/// A module explicitly prepared through the Engine API, replayed before VM compilation.
/// The VM needs their AST declarations present while it statically resolves the user's query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VmModulePrelude {
    Include(String),
    /// Module name and optional `as` alias.
    Import(String, Option<String>),
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
            vm_module_prelude: Vec::new(),
            #[cfg(feature = "mqc")]
            mqc_programs: Default::default(),
        }
    }
}

impl<IO: Io> Engine<DefaultModuleResolver, IO> {
    /// Creates an engine with the default module resolver and a shared [`Io`].
    ///
    /// The same `io` handles builtins and local `include`/`import` statements, so
    /// filesystem access follows one permission policy. Standard modules remain available.
    /// Use [`Engine::with_io`] when supplying a custom module resolver.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mq_lang::{Engine, NativeIo, SandboxedIo, Shared};
    ///
    /// let io = Shared::new(SandboxedIo::new(NativeIo::default()));
    /// let mut engine = Engine::with_default_io(io);
    /// engine.load_builtin_module();
    /// ```
    pub fn with_default_io(io: Shared<IO>) -> Self {
        let resolver = DefaultModuleResolver::with_io(Shared::clone(&io) as Shared<dyn Io>, vec![]);
        Self::with_io(resolver, io)
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
            vm_module_prelude: Vec::new(),
            #[cfg(feature = "mqc")]
            mqc_programs: Default::default(),
        }
    }

    /// Set the maximum call stack depth for function calls.
    ///
    /// This prevents infinite recursion by limiting how deep function
    /// calls can be nested. Useful for controlling resource usage. The default is 10,000 in
    /// release builds (256 in debug builds).
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

    /// Enables traces for uncaught VM errors.
    pub fn set_capture_stack_trace(&mut self, enabled: bool) {
        self.vm.options.capture_stack_trace = enabled;
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
        self.define_value_unchecked(name, RuntimeValue::String(Shared::new(value.to_string())));
    }

    /// Defines several string variables and publishes one VM global snapshot.
    ///
    /// Useful when updating related values, such as the file path variables for each input.
    ///
    /// ```
    /// let engine = mq_lang::DefaultEngine::default();
    /// engine.define_string_values(&[("__FILE__", "input.md"), ("__FILE_NAME__", "input.md")]);
    /// ```
    pub fn define_string_values(&self, values: &[(&str, &str)]) {
        self.vm.define_many(values.iter().map(|(name, value)| {
            (
                Ident::new(name),
                RuntimeValue::String(Shared::new((*value).to_string())),
            )
        }));
    }

    /// Defines an arbitrary runtime value in the current environment.
    ///
    /// Values that retain VM execution state, such as coroutines and closures, cannot be
    /// injected. They may contain bytecode and source locations owned by another evaluation.
    pub fn define_value(&self, name: &str, value: RuntimeValue) -> Result<(), DefineValueError> {
        if let Some(kind) = value.vm_bound_value_kind() {
            return Err(DefineValueError::VmBoundValue(kind));
        }
        self.define_value_unchecked(name, value);
        Ok(())
    }

    fn define_value_unchecked(&self, name: &str, value: RuntimeValue) {
        self.vm.define(Ident::new(name), value);
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
            .push(VmModulePrelude::Import(module_name.to_string(), None));
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
    /// The `Engine` manages parsing and evaluation of mq.
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
        let token_arena = self.query_token_arena();
        let program = parse(code, Shared::clone(&token_arena))?;

        #[cfg(feature = "debugger")]
        self.vm.module_loader.set_source_code(code.to_string());

        let compiled = CompiledProgram::cached(code.to_string(), program);
        self.run_compiled(&compiled, input, token_arena, None)
            .map_err(|cause| Box::new(error::Error::from_error(code, cause, self.vm.module_loader.clone())))
    }

    /// Compiles mq code into a [`CompiledProgram`] that can be evaluated multiple times.
    ///
    /// Use this with `eval_compiled` to avoid re-parsing the same query for each input.
    pub fn compile(&mut self, code: &str) -> Result<CompiledProgram, Box<error::Error>> {
        if code.is_empty() {
            return Ok(CompiledProgram::cached(String::new(), Vec::new()));
        }
        let _io_guard = io_context::scoped(Shared::clone(&self.vm.io) as Shared<dyn Io>);
        let program = parse(code, Shared::clone(&self.token_arena))?;
        Ok(CompiledProgram::cached(code.to_string(), program))
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

    /// Lists the Tarn bytecode that would be executed for `compiled`, without running it.
    ///
    /// Available only with the `debug-trace` feature, for tools such as `mq-dbg`.
    #[cfg(feature = "debug-trace")]
    pub fn dump_bytecode(&mut self, compiled: &CompiledProgram) -> Result<crate::BytecodeDump, Box<error::Error>> {
        self.vm.module_loader.set_source_code(compiled.source.clone());
        #[allow(clippy::infallible_destructuring_match)]
        let program = match &compiled.body {
            ProgramBody::Ast(program) => program,
            #[cfg(feature = "mqc")]
            ProgramBody::Precompiled(precompiled) => {
                return Ok(tarn::dump_compiled_program(precompiled, &self.token_arena));
            }
        };
        let global_bindings = self.vm.global_bindings_snapshot();
        let vm_program = tarn::build_program(program, Shared::clone(&self.token_arena), &self.vm_module_prelude)?;
        let vm_program = vm_program.as_ref().unwrap_or(program);

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
        self.run_compiled(compiled, input, Shared::clone(&self.token_arena), None)
            .map_err(|cause| {
                Box::new(error::Error::from_error(
                    &compiled.source,
                    cause,
                    self.vm.module_loader.clone(),
                ))
            })
    }

    /// The arena for one `eval` query's tokens, freed once it has run. A debugger keeps them,
    /// since it reads them later.
    fn query_token_arena(&self) -> TokenArena {
        if cfg!(feature = "debugger") {
            Shared::clone(&self.token_arena)
        } else {
            layered_token_arena(&self.token_arena)
        }
    }

    /// Runs `compiled`, resolving its tokens in `token_arena` and carrying `session`'s bindings.
    pub(crate) fn run_compiled(
        &mut self,
        compiled: &CompiledProgram,
        input: impl Iterator<Item = RuntimeValue>,
        token_arena: TokenArena,
        session: Option<&SessionBindings>,
    ) -> Result<RuntimeValues, error::InnerError> {
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
        let vm_program = compiled
            .program()
            .map(|program| tarn::build_program(program, Shared::clone(&token_arena), &self.vm_module_prelude))
            .transpose()
            .map_err(|error| error.cause)?
            .flatten();

        let (timeout, max_call_stack_depth, capture_stack_trace) = (
            self.vm.options.timeout,
            self.vm.options.max_call_stack_depth,
            self.vm.options.capture_stack_trace,
        );
        let module_loader = self.vm.module_loader.with_same_resolver();

        let vm = tarn::TarnVm {
            engine: tarn::EngineRunContext {
                host_functions: &host_functions,
                timeout,
                max_call_stack_depth,
                capture_stack_trace,
                token_arena: Shared::clone(&token_arena),
                module_loader,
                global_bindings: &global_bindings,
                session,
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
        vm.run(
            compiled,
            #[cfg(feature = "debugger")]
            vm_program.as_ref(),
            input,
        )
        .map(Into::into)
        .map_err(|error| error.into_inner_error(token_arena))
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
mod tests;
