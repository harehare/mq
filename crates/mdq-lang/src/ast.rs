use std::rc::Rc;

use compact_str::CompactString;
use node::Node;

pub mod error;
pub mod node;
pub mod parser;

pub type Program = Vec<Rc<Node>>;
pub type Params = Vec<Rc<Node>>;
pub type IdentName = CompactString;
