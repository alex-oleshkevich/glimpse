pub mod cli;
pub(crate) mod client;
mod dispatcher;
pub(crate) mod protocol;
pub(crate) mod server;

pub use server::{IpcHandle, IpcServer};

#[cfg(test)]
mod tests;
