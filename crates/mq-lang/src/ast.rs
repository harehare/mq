use std::rc::Rc;

use compact_str::CompactString;

use crate::{Token, arena::ArenaId};

pub mod error;
pub mod node;
pub mod parser;
pub mod expr_ref;
pub mod pool;
pub mod range_utils;

pub type Program = Vec<ExprRef>; // Changed from Vec<Rc<Node>>
pub type IdentName = CompactString;
pub type TokenId = ArenaId<Rc<Token>>;

pub use expr_ref::ExprRef;
pub use pool::ExprPool;
pub use range_utils::get_expr_range;
