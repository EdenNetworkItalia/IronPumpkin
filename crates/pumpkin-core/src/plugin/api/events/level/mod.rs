//! The `NeoForge` level, block and tick events at fire sites where Pumpkin has no event.
//!
//! The packages are `event.level`, `event.level.block` and `event.tick`. Where Pumpkin has an
//! event at the fire site, the `NeoForge` event maps to it (`mapping-table.md`).

pub mod block;

pub use block::*;
