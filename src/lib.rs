//! Molt library crate.
//!
//! The binary (`src/main.rs`) is a thin wrapper around [`cli::run`]. Exposing
//! the modules as a library keeps the layers testable in isolation (see
//! `tests/logic.rs`) and makes the domain reusable.

pub mod backend;
pub mod cli;
pub mod config;
pub mod domain;
pub mod error;
pub mod keymap;
pub mod pyfinder;
pub mod registry;
pub mod runner;
pub mod service;
pub mod theme;
pub mod tui;
