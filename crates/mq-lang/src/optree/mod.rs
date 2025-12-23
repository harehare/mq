//! OpTree (Flat AST) module for mq.
//!
//! OpTree is a flattened representation of the Abstract Syntax Tree (AST) that uses
//! array indices instead of pointers for better performance and memory locality.
//!
//! # Architecture
//!
//! ```text
//! Source Code
//!     ↓
//! Parser
//!     ↓
//! AST (tree structure with Shared<Node>)
//!     ↓
//! OpTreeTransformer
//!     ↓
//! OpTree (OpPool + SourceMap)
//!     ↓
//! OpTreeEvaluator
//!     ↓
//! RuntimeValue
//! ```
//!
//! # Benefits
//!
//! - **Better memory locality**: All instructions stored in contiguous array
//! - **Smaller references**: 32-bit indices vs 64-bit pointers (50% smaller)
//! - **Simpler lifetimes**: Single OpPool lifetime instead of per-node lifetimes
//! - **Easier serialization**: Flat structure is trivial to serialize
//!
//! # Example
//!
//! ```rust,ignore
//! use mq_lang::optree::OpTreeTransformer;
//! use mq_lang::parse;
//!
//! let code = "1 + 2";
//! let program = parse(code, token_arena)?;
//!
//! // Transform AST to OpTree
//! let transformer = OpTreeTransformer::new();
//! let (pool, source_map, root) = transformer.transform(&program);
//!
//! // Evaluate OpTree
//! let mut evaluator = OpTreeEvaluator::new(pool, source_map, ...);
//! let result = evaluator.eval(root, runtime_value)?;
//! ```

pub mod debug;
mod eval;
mod instruction;
mod transform;

pub use debug::dump_optree;
pub use eval::OpTreeEvaluator;
pub use instruction::{AccessTarget, MatchArm, Op, OpPool, OpRef, SourceMap, StringSegment};
pub use transform::OpTreeTransformer;
