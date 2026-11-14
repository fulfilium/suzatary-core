use super::StoreError;
use suzatary_base::refusal::Refusal;

/// What an [`Log::edit`] does to a log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit<E> {
    /// One line on the end.
    Push(E),
    /// The whole log replaced by these lines at once; the one write that removes text.
    Replace(Vec<E>),
    /// Nothing written.
    Keep,
}

/// What an [`Log::edit`] asks: the edit to make, given the log's lines.
pub type Decide<'a, E> = dyn FnMut(&[E]) -> Result<Edit<E>, Refusal> + 'a;

/// Append-only lines, one log per key; a single log is keyed by `()`.
pub trait Log<K, E>: Sync {
    /// Every key that has a log, sorted.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn keys(&self) -> Result<Vec<K>, StoreError>;

    /// One log's lines, in the order written.
    ///
    /// # Errors
    /// [`StoreError::Absent`] for a log never started, and [`StoreError::Unreadable`].
    fn lines(&self, key: &K) -> Result<Vec<E>, StoreError>;

    /// One log's lines, none for a log never started.
    ///
    /// # Errors
    /// [`StoreError::Unreadable`].
    fn lines_if_any(&self, key: &K) -> Result<Vec<E>, StoreError> {
        match self.lines(key) {
            Err(StoreError::Absent { .. }) => Ok(Vec::new()),
            read => read,
        }
    }

    /// Start a log with its first line.
    ///
    /// # Errors
    /// [`StoreError::Taken`] for a key that has one, and [`StoreError::Unwritable`].
    fn start(&self, key: &K, first: &E) -> Result<(), StoreError>;

    /// Push one line onto a started log.
    ///
    /// # Errors
    /// [`StoreError::Absent`] for a log never started, and [`StoreError::Unwritable`].
    fn push(&self, key: &K, line: &E) -> Result<(), StoreError>;

    /// Under the log's lock, ask `decide` about its lines (none for a log never started) and
    /// perform the edit it answers, starting the log if need be.
    ///
    /// # Errors
    /// `decide`'s refusal, with nothing written, and the store's own.
    fn edit(&self, key: &K, decide: &mut Decide<'_, E>) -> Result<Edit<E>, Refusal>;
}
