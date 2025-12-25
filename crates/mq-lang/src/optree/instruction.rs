//! OpTree instruction types and storage.
//!
//! This module defines the flattened instruction set and storage mechanisms
//! for the OpTree (Flat AST) representation.

use crate::{
    Ident, Shared,
    ast::{
        TokenId,
        node::{Literal, Pattern},
    },
    selector::Selector,
};
use smallvec::SmallVec;
use smol_str::SmolStr;
use std::fmt;

/// Reference to an instruction in the OpPool.
///
/// OpRef is a 32-bit index into the OpPool's instruction array.
/// This is half the size of a 64-bit pointer, improving memory efficiency.
///
/// # Example
///
/// ```rust,ignore
/// let op_ref = pool.alloc(Op::Literal(Literal::Number(42.0)));
/// let op = pool.get(op_ref);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OpRef(u32);

impl OpRef {
    /// Creates a new OpRef from a u32 index.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the underlying index.
    #[inline]
    pub const fn id(self) -> u32 {
        self.0
    }
}

impl fmt::Display for OpRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpRef({})", self.0)
    }
}

/// A contiguous pool storing all OpTree instructions.
///
/// OpPool provides efficient storage and retrieval of flattened AST instructions.
/// All instructions are stored in a single Vec for optimal memory locality.
///
/// # Example
///
/// ```rust,ignore
/// let mut pool = OpPool::new();
/// let ref1 = pool.alloc(Op::Literal(Literal::Number(1.0)));
/// let ref2 = pool.alloc(Op::Literal(Literal::Number(2.0)));
/// let add_ref = pool.alloc(Op::Call {
///     name: "add".into(),
///     args: smallvec![ref1, ref2],
/// });
/// ```
#[derive(Debug, Clone)]
pub struct OpPool {
    instructions: Vec<Shared<Op>>,
}

impl OpPool {
    /// Creates a new empty OpPool with default capacity.
    pub fn new() -> Self {
        Self::with_capacity(256)
    }

    /// Creates a new OpPool with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            instructions: Vec::with_capacity(capacity),
        }
    }

    /// Allocates an instruction in the pool and returns its reference.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let mut pool = OpPool::new();
    /// let op_ref = pool.alloc(Op::Literal(Literal::Number(42.0)));
    /// ```
    pub fn alloc(&mut self, op: Op) -> OpRef {
        let id = self.instructions.len() as u32;
        self.instructions.push(Shared::new(op));
        OpRef::new(id)
    }

    /// Retrieves an instruction by its reference.
    ///
    /// # Panics
    ///
    /// Panics if the OpRef is invalid (out of bounds).
    #[inline(always)]
    pub fn get(&self, op_ref: OpRef) -> &Shared<Op> {
        &self.instructions[op_ref.id() as usize]
    }

    /// Returns the number of instructions in the pool.
    #[inline]
    pub fn len(&self) -> usize {
        self.instructions.len()
    }

    /// Returns true if the pool is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.instructions.is_empty()
    }

    /// Returns an iterator over all instructions with their OpRefs.
    pub fn iter(&self) -> impl Iterator<Item = (OpRef, &Shared<Op>)> {
        self.instructions
            .iter()
            .enumerate()
            .map(|(i, op)| (OpRef::new(i as u32), op))
    }
}

impl Default for OpPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps OpRef to source location (TokenId) for error reporting.
///
/// SourceMap maintains a parallel array to OpPool, mapping each instruction
/// to its original source location. This enables accurate error messages
/// without storing TokenId in every Op variant.
///
/// # Example
///
/// ```rust,ignore
/// let mut source_map = SourceMap::new();
/// let op_id = source_map.register(token_id);
/// let op_ref = OpRef::new(op_id);
/// // Later, retrieve source location for error reporting
/// let token_id = source_map.get(op_ref);
/// ```
#[derive(Debug, Clone)]
pub struct SourceMap {
    /// Maps instruction index to TokenId
    locations: Vec<TokenId>,
}

impl SourceMap {
    /// Creates a new empty SourceMap with default capacity.
    pub fn new() -> Self {
        Self::with_capacity(256)
    }

    /// Creates a new SourceMap with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            locations: Vec::with_capacity(capacity),
        }
    }

    /// Registers a TokenId and returns the index that will be used for the next OpRef.
    ///
    /// This should be called before allocating the corresponding Op in OpPool.
    pub fn register(&mut self, token_id: TokenId) -> u32 {
        let id = self.locations.len() as u32;
        self.locations.push(token_id);
        id
    }

    /// Retrieves the TokenId for a given OpRef.
    ///
    /// # Panics
    ///
    /// Panics if the OpRef is invalid (out of bounds).
    #[inline(always)]
    pub fn get(&self, op_ref: OpRef) -> TokenId {
        self.locations[op_ref.id() as usize]
    }

    /// Returns the number of registered locations.
    #[inline]
    pub fn len(&self) -> usize {
        self.locations.len()
    }

    /// Returns true if no locations have been registered.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.locations.is_empty()
    }
}

impl Default for SourceMap {
    fn default() -> Self {
        Self::new()
    }
}

/// String interpolation segment in flattened form.
///
/// This mirrors `ast::node::StringSegment` but uses OpRef instead of Shared<Node>.
#[derive(Debug, Clone, PartialEq)]
pub enum StringSegment {
    /// Plain text segment
    Text(String),
    /// Expression to be evaluated (reference to op in pool)
    Expr(OpRef),
    /// Environment variable reference
    Env(SmolStr),
    /// Self reference
    Self_,
}

/// Pattern match arm in flattened form.
///
/// This mirrors `ast::node::MatchArm` but uses OpRef instead of Shared<Node>.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// Pattern to match against
    pub pattern: Pattern,
    /// Optional guard condition (reference to op in pool)
    pub guard: Option<OpRef>,
    /// Body to execute on match (reference to op in pool)
    pub body: OpRef,
}

/// Qualified access target in flattened form.
///
/// This mirrors `ast::node::AccessTarget` but uses OpRef and Ident.
#[derive(Debug, Clone, PartialEq)]
pub enum AccessTarget {
    /// Function call: Module::function(args)
    Call(Ident, SmallVec<[OpRef; 8]>),
    /// Identifier access: Module::value
    Ident(Ident),
}

/// OpTree instruction set - flattened representation of AST expressions.
///
/// Each Op variant represents a language construct, using OpRef to reference
/// child instructions instead of pointer-based tree structures.
///
/// # Design
///
/// - All child nodes are referenced via OpRef (32-bit index)
/// - Source location tracking is external (via SourceMap)
/// - Instructions are stored contiguously in OpPool for cache efficiency
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Literal value (number, string, bool, symbol, none)
    Literal(Literal),
    /// Variable identifier reference
    Ident(Ident),
    /// Self reference (current value in pipeline)
    Self_,
    /// Reference to all input nodes
    Nodes,
    /// Immutable variable binding: let name = value;
    Let {
        /// Variable name
        name: Ident,
        /// Value expression (reference to op in pool)
        value: OpRef,
    },
    /// Mutable variable declaration: var name = value;
    Var {
        /// Variable name
        name: Ident,
        /// Initial value expression (reference to op in pool)
        value: OpRef,
    },
    /// Variable assignment: name = value;
    Assign {
        /// Variable name
        name: Ident,
        /// New value expression (reference to op in pool)
        value: OpRef,
    },
    /// Conditional expression: if cond: body; elif cond2: body2; else: body3;
    If {
        /// Branches: (optional condition, body)
        /// First branch with None condition is else clause
        branches: SmallVec<[(Option<OpRef>, OpRef); 8]>,
    },
    /// While loop: while condition: body;
    While {
        /// Loop condition (reference to op in pool)
        condition: OpRef,
        /// Loop body (reference to op in pool)
        body: OpRef,
    },
    /// For-each loop: foreach name in iterator: body;
    Foreach {
        /// Loop variable name
        name: Ident,
        /// Iterator expression (reference to op in pool)
        iterator: OpRef,
        /// Loop body (reference to op in pool)
        body: OpRef,
    },
    /// Pattern matching: match value: case pattern: body; ...
    Match {
        /// Value to match (reference to op in pool)
        value: OpRef,
        /// Match arms with patterns, optional guards, and bodies
        arms: SmallVec<[MatchArm; 8]>,
    },
    /// Break from loop
    Break,
    /// Continue to next loop iteration
    Continue,
    /// Function definition: def name(params): body;
    Def {
        /// Function name
        name: Ident,
        /// Parameter names (references to op in pool)
        params: SmallVec<[OpRef; 8]>,
        /// Function body (reference to op in pool)
        body: OpRef,
    },
    /// Anonymous function: fn(params): body;
    Fn {
        /// Parameter names (references to op in pool)
        params: SmallVec<[OpRef; 8]>,
        /// Function body (reference to op in pool)
        body: OpRef,
    },
    /// Static function call: function_name(args)
    Call {
        /// Function name
        name: Ident,
        /// Arguments (references to ops in pool)
        args: SmallVec<[OpRef; 8]>,
    },
    /// Dynamic function call: expr(args)
    CallDynamic {
        /// Callable expression (reference to op in pool)
        callable: OpRef,
        /// Arguments (references to ops in pool)
        args: SmallVec<[OpRef; 8]>,
    },
    /// Code block with its own scope: { ... }
    Block(OpRef),
    /// Sequential execution of multiple operations
    Sequence(SmallVec<[OpRef; 8]>),
    /// Logical AND: expr1 and expr2
    And(OpRef, OpRef),
    /// Logical OR: expr1 or expr2
    Or(OpRef, OpRef),
    /// Parenthesized expression: (expr)
    Paren(OpRef),
    /// Interpolated string: "text {expr} more"
    InterpolatedString(Vec<StringSegment>),
    /// Markdown selector: .heading, .list, etc.
    Selector(Selector),
    /// Module qualified access: Module::function or Module::value
    QualifiedAccess {
        /// Module path (e.g., ["Std", "Array"])
        module_path: Vec<Ident>,
        /// Access target (function call or identifier)
        target: AccessTarget,
    },
    /// Module definition: module name: body;
    Module {
        /// Module name
        name: Ident,
        /// Module body (reference to op in pool)
        body: OpRef,
    },
    /// Include external file: include "path.mq"
    Include(Literal),
    /// Import module: import "module"
    Import(Literal),
    /// Try-catch expression: try expr catch handler
    Try {
        /// Expression to try (reference to op in pool)
        try_expr: OpRef,
        /// Error handler (reference to op in pool)
        catch_expr: OpRef,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::ArenaId;

    #[test]
    fn test_opref_creation() {
        let op_ref = OpRef::new(42);
        assert_eq!(op_ref.id(), 42);
    }

    #[test]
    fn test_oppool_alloc_and_get() {
        let mut pool = OpPool::new();

        let ref1 = pool.alloc(Op::Literal(Literal::Number(42.0.into())));
        let ref2 = pool.alloc(Op::Literal(Literal::String("hello".to_string())));
        let ref3 = pool.alloc(Op::Ident("x".into()));

        assert_eq!(pool.len(), 3);
        assert!(!pool.is_empty());

        assert!(matches!(pool.get(ref1).as_ref(), Op::Literal(Literal::Number(_))));
        assert!(matches!(
            pool.get(ref2).as_ref(),
            Op::Literal(Literal::String(s)) if s == "hello"
        ));
        assert!(matches!(pool.get(ref3).as_ref(), Op::Ident(name) if name.as_str() == "x"));
    }

    #[test]
    fn test_oppool_iter() {
        let mut pool = OpPool::new();
        pool.alloc(Op::Literal(Literal::Number(1.0.into())));
        pool.alloc(Op::Literal(Literal::Number(2.0.into())));

        let ops: Vec<_> = pool.iter().collect();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].0.id(), 0);
        assert_eq!(ops[1].0.id(), 1);
    }

    #[test]
    fn test_source_map_register_and_get() {
        let mut source_map = SourceMap::new();

        let token_id1 = ArenaId::new(10);
        let token_id2 = ArenaId::new(20);

        let id1 = source_map.register(token_id1);
        let id2 = source_map.register(token_id2);

        assert_eq!(source_map.len(), 2);
        assert!(!source_map.is_empty());

        let op_ref1 = OpRef::new(id1);
        let op_ref2 = OpRef::new(id2);

        assert_eq!(source_map.get(op_ref1), token_id1);
        assert_eq!(source_map.get(op_ref2), token_id2);
    }

    #[test]
    fn test_oppool_with_capacity() {
        let pool = OpPool::with_capacity(100);
        assert_eq!(pool.instructions.capacity(), 100);
        assert!(pool.is_empty());
    }

    #[test]
    fn test_source_map_with_capacity() {
        let source_map = SourceMap::with_capacity(100);
        assert_eq!(source_map.locations.capacity(), 100);
        assert!(source_map.is_empty());
    }
}
