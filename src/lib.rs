//! A building as a parameter graph in plain text.
//!
//! The model is data, not code: named parameters with a grade saying where each
//! number came from, rules (derived values), boxes whose coordinates are
//! expressions, and checks against independent sources. Because it is text,
//! git versions it; because it is a graph, a diff can say which parameter moved
//! which element.

pub mod diff;
pub mod expr;
pub mod merge;
pub mod model;
pub mod obj;
