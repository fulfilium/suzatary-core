use super::StoreError;
use suzatary_base::refusal::Refusal;

/// What an [`Log::edit`] does to a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit<E> {
    Push(E),
    /// The one write that removes text.
    Replace(Vec<E>),
    Keep,
}

/// What an [`Log::edit`] asks: the edit to make, given the log's lines.
pub type Decide<'a, E> = dyn FnMut(&[E]) -> Result<Edit<E>, Refusal> + 'a;

/// Append-only lines, one log per key; a single log is keyed by `()`.
pub trait Log<K, E>: Sync {
    /// Every key that has a log, sorted.
    fn keys(&self) -> Result<Vec<K>, StoreError>;

    /// One log's lines, in the order written.
    ///
    /// # Errors
    /// [`StoreError::Absent`] for a log never started.
    fn lines(&self, key: &K) -> Result<Vec<E>, StoreError>;

    /// One log's lines, none for a log never started.
    fn lines_if_any(&self, key: &K) -> Result<Vec<E>, StoreError> {
        match self.lines(key) {
            Err(StoreError::Absent { .. }) => Ok(Vec::new()),
            read => read,
        }
    }

    /// # Errors
    /// [`StoreError::Taken`] for a key that has one.
    fn start(&self, key: &K, first: &E) -> Result<(), StoreError>;

    /// # Errors
    /// [`StoreError::Absent`] for a log never started.
    fn push(&self, key: &K, line: &E) -> Result<(), StoreError>;

    /// Under the log's lock, ask `decide` about its lines and perform the edit it answers.
    ///
    /// # Errors
    /// `decide`'s refusal, with nothing written, and the store's own.
    fn edit(&self, key: &K, decide: &mut Decide<'_, E>) -> Result<Edit<E>, Refusal>;
}
