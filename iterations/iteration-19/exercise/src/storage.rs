//! 表の行をバイト列にして，ページに置く．ページはディスクかメモリーに保存する．

pub mod buffer;
pub mod disk;
pub mod heap;
pub mod page;
pub mod tuple;
