#![allow(dead_code)] // TODO: Cleanup
#![cfg_attr(feature = "cargo-clippy", allow(inline_always))] // I know what I'm doing
#![feature(catch_expr, str_checked_slicing, try_from, const_fn, associated_type_defaults)]
extern crate seahash;
extern crate string_cache;
extern crate phf;
extern crate serde;
extern crate rmp_serde;
extern crate serde_bytes;
extern crate byteorder;
#[macro_use]
extern crate serde_derive;
extern crate typed_arena;
extern crate walkdir;
extern crate chan;
extern crate serde_json;
extern crate ordermap;
extern crate crossbeam;
extern crate git2;
extern crate curl;
extern crate zip;
extern crate regex;
#[macro_use]
extern crate lazy_static;
extern crate csv;
extern crate lz4;
extern crate parking_lot;
extern crate thread_local;
extern crate chashmap;
#[macro_use]
extern crate log;
extern crate chrono;

pub mod utils;
pub mod minecraft;
pub mod ranges;
//pub mod classfile;
pub mod types;
pub mod mappings;
