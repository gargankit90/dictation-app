//! Speech-to-text engines behind the Transcriber port.

#![forbid(unsafe_code)]

pub mod check_key;
pub mod cleanup;
pub mod connect;
pub mod engine;
mod http;
pub mod llama;
pub mod local;
pub mod mock;
pub mod models;
pub mod openrouter;
