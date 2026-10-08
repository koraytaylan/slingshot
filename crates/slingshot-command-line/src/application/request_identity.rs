//! Injected invocation and cooperative producer identity.

/// Identity facts supplied by the invoking process.
pub trait RequestIdentityBoundary {
    /// Returns one new identity shared across the invocation's phases.
    fn invent_request_identifier(&self) -> String;

    /// Returns the opaque producer identity, or the shared default queue.
    ///
    /// # Errors
    ///
    /// Refuses an invalid process label without including its contents.
    fn producer_identity(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
}
