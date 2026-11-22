/// How many random bytes salt a text.
pub const SALT_BYTES: usize = 16;

/// The system's random generator, a capability so a test can draw known bytes.
pub trait Entropy: Sync {
    /// Fill `bytes` with bytes nobody can guess.
    fn fill(&self, bytes: &mut [u8]) -> Result<(), String>;

    fn salt(&self) -> Result<[u8; SALT_BYTES], String> {
        let mut salt = [0; SALT_BYTES];
        self.fill(&mut salt)?;
        Ok(salt)
    }
}
