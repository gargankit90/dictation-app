//! Speech-to-text engines behind the Transcriber port.

#![forbid(unsafe_code)]

pub mod check_key;
pub mod engine;
mod http;
pub mod mock;
pub mod openai_compat;
