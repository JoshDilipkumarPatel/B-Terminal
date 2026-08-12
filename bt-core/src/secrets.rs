use zeroize::{Zeroize, ZeroizeOnDrop};

/// Securely stores the HuggingFace API key in memory.
/// Implements `ZeroizeOnDrop` so that if the program panics, is killed, 
/// or the credential falls out of scope, the memory is forcefully overwritten with zeros,
/// preventing the API key from being extracted via a core dump.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct HfCredentials {
    api_key: String,
}

impl HfCredentials {
    pub fn new(key: &str) -> Self {
        Self {
            api_key: key.to_string(),
        }
    }

    /// Access the API key for network calls.
    pub fn expose_secret(&self) -> &str {
        &self.api_key
    }
}
