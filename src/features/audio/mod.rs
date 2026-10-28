//! Core Audio feature: endpoint discovery, volume tracking and restoration.

mod com;
mod endpoint;
mod engine;
mod notifier;
mod policy;
mod request;

pub use engine::run;
pub use request::AudioRequest;
