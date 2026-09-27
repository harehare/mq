//! [`CompiledProgram`]: a query compiled once and evaluated many times.
#[cfg(not(feature = "debugger"))]
use crate::SharedCell;
#[cfg(any(not(feature = "debugger"), feature = "mqc"))]
use crate::{Shared, tarn};

/// A compiled mq program bundled with its original source, returned by [`Engine::compile`].
#[derive(Debug, Clone)]
pub struct CompiledProgram {
    pub(crate) source: String,
    pub(crate) body: ProgramBody,
}

#[derive(Debug, Clone)]
pub(crate) enum ProgramBody {
    /// Compiled with debugger instrumentation on each run.
    #[cfg(feature = "debugger")]
    Ast(crate::ast::Program),
    /// Bytecode is built on the first run and reused.
    #[cfg(not(feature = "debugger"))]
    Cached {
        program: crate::ast::Program,
        cache: VmCache,
    },
    /// Loaded from a `.mqc` file.
    #[cfg(feature = "mqc")]
    Precompiled(Shared<tarn::split_program::SplitProgram>),
}

/// Bytecode built from a [`ProgramBody::Cached`] program, shared by its clones.
#[cfg(not(feature = "debugger"))]
#[derive(Debug, Clone, Default)]
pub(crate) struct VmCache(Shared<SharedCell<Option<Shared<tarn::CachedProgram>>>>);

#[cfg(not(feature = "debugger"))]
impl VmCache {
    pub(crate) fn get(&self) -> Option<Shared<tarn::CachedProgram>> {
        #[cfg(feature = "sync")]
        {
            self.0.read().unwrap().clone()
        }
        #[cfg(not(feature = "sync"))]
        {
            self.0.borrow().clone()
        }
    }

    pub(crate) fn set(&self, program: Shared<tarn::CachedProgram>) {
        #[cfg(feature = "sync")]
        {
            *self.0.write().unwrap() = Some(program);
        }
        #[cfg(not(feature = "sync"))]
        {
            *self.0.borrow_mut() = Some(program);
        }
    }
}

impl CompiledProgram {
    /// Wraps `program` so its bytecode is cached across runs.
    pub(super) fn cached(source: String, program: crate::ast::Program) -> Self {
        #[cfg(not(feature = "debugger"))]
        let body = ProgramBody::Cached {
            program,
            cache: VmCache::default(),
        };
        #[cfg(feature = "debugger")]
        let body = ProgramBody::Ast(program);
        Self { source, body }
    }

    /// Returns the original source code.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the underlying AST nodes, or `None` for a program loaded from a `.mqc` file.
    pub fn program(&self) -> Option<&crate::ast::Program> {
        match &self.body {
            #[cfg(feature = "debugger")]
            ProgramBody::Ast(program) => Some(program),
            #[cfg(not(feature = "debugger"))]
            ProgramBody::Cached { program, .. } => Some(program),
            #[cfg(feature = "mqc")]
            ProgramBody::Precompiled(_) => None,
        }
    }

    #[cfg(feature = "mqc")]
    pub(crate) fn from_precompiled(source: String, program: tarn::split_program::SplitProgram) -> Self {
        Self {
            source,
            body: ProgramBody::Precompiled(Shared::new(program)),
        }
    }

    #[cfg(all(test, not(feature = "debugger")))]
    pub(crate) fn vm_cache(&self) -> Option<&VmCache> {
        match &self.body {
            ProgramBody::Cached { cache, .. } => Some(cache),
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }
}

impl From<crate::ast::Program> for CompiledProgram {
    /// Wraps a raw `Program` (e.g. from `ast_from_json`) with no source context.
    fn from(program: crate::ast::Program) -> Self {
        Self::cached(String::new(), program)
    }
}
