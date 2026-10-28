//! Model output as CommonMark: the blocks the panel draws ([`blocks`]) and
//! the selection arithmetic over them ([`select`]).

mod blocks;
pub mod select;
pub use blocks::{Block, Run, Style, blocks};
