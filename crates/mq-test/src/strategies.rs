//! Property-based testing strategies for mq.
//!
//! This module provides reusable proptest strategies for generating
//! various types of mq expressions. These strategies can be used across
//! different test modules (optimizer, parser, evaluator, etc.) to ensure
//! consistent testing patterns.
//!
//! # Examples
//!
//! ```rust,ignore
//! use mq_test::strategies::expr::*;
//! use proptest::prelude::*;
//!
//! proptest! {
//!     #[test]
//!     fn test_something(expr in arb_arithmetic_expr()) {
//!         // Your test here
//!     }
//! }
//! ```

pub mod expr;
