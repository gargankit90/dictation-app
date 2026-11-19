//! Speech-to-text engines behind the Transcriber port.

#![forbid(unsafe_code)]

pub mod check_key;
pub mod engine;
pub mod groq_cleanup;
pub mod groq_cleanup_prompt;
mod http;
pub mod local;
pub mod mock;
pub mod models;
pub mod openai_compat;
