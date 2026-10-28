//! What a surface tells its model each shared verb does.

/// The description of each verb [`super::Holds`] shares, one per surface: one implementation, and
/// each model told only what its own schema offers.
pub struct Says {
    /// `list_open_todos`.
    pub list_open_todos: &'static str,
    /// `git_diff_summary`.
    pub git_diff_summary: &'static str,
    /// `history`.
    pub history: &'static str,
    /// `todo_show`.
    pub todo_show: &'static str,
    /// `show`.
    pub show: &'static str,
    /// `record`.
    pub record: &'static str,
    /// `todo_add`.
    pub todo_add: &'static str,
    /// `todo_update`.
    pub todo_update: &'static str,
    /// `zoom`.
    pub zoom: &'static str,
}
