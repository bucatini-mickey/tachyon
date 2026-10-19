//! Open water at sea scale: the sea as smooth fields (tide, currents, waves,
//! whirlpools), ships as scenes posed on it (`craft`), round shot
//! (`ballistics`) and seeing over the horizon (`sight`).

mod field;
pub use field::*;

pub mod ballistics;
pub mod craft;
pub mod sight;
