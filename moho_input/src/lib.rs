//! Engine input vocabulary shared by the binary and ui crates: platform-free
//! keys, and action-keyed bindings that persist by name.

pub mod bindings;
pub mod key;

pub use bindings::Action;
