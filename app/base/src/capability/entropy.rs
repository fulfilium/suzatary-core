//! Fresh random bytes, as a capability rather than a call.

/// How many random bytes salt a text.
pub const SALT_BYTES: usize = 16;

/// The system's random generator, taken as a capability so a test can draw known bytes. A salt
/// and a key are both drawn from it, so one source answers for every byte nobody may guess.
pub trait Entropy: Sync {
    /// Fill `bytes` with bytes nobody can guess.
    ///
    /// # Errors
    /// The generator's failure, as a sentence.
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String>;

    /// A salt nobody can guess.
    ///
    /// # Errors
    /// The generator's failure, as a sentence.
    fn salt(&self) -> Result<[u8; SALT_BYTES], String> {
        let mut salt = [0; SALT_BYTES];
        self.fill(&mut salt)?;
        Ok(salt)
    }
}
