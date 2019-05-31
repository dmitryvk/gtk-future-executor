//! This crate provides basic building blocks for writing async GUI code with Gtk-rs:
//! 1. `GtkEventLoopAsyncExecutor` - an executor for executing futures that may manipulate GUI widgets
//! 2. `Promise` - an implementation of [futures::Future] that is often useful for GUI code

mod executor;
mod promise;

pub use executor::GtkEventLoopAsyncExecutor;
pub use promise::Promise;

