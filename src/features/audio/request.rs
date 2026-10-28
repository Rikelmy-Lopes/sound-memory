//! Requests the rest of the application can send to the audio engine.

/// Commands understood by the engine loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AudioRequest {
    /// Unregister callbacks, flush the store and stop the loop.
    Shutdown,
    /// Re-apply every saved level to the connected devices.
    RestoreAll,
}
