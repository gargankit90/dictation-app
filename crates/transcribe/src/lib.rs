//! Speech-to-text engines behind the Transcriber port.

#![forbid(unsafe_code)]

pub mod assemblyai;
pub mod assemblyai_prompt;
pub mod check_key;
pub mod engine;
mod http;
pub mod local;
pub mod mock;
pub mod models;
pub mod openai_compat;
