//! The only code that touches the terminal: raw mode and the diffing writer
//! (`term`), backend-neutral input events (`input`), and the main loop (`app`).

pub mod app;
pub mod input;
pub mod term;
