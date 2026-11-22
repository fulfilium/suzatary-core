/// The description of each verb [`super::Holds`] shares, one per surface: one implementation, and
/// each model told only what its own schema offers.
pub struct Says {
    pub list_open_todos: &'static str,
    pub(crate) git_diff_summary: &'static str,
    pub history: &'static str,
    pub todo_show: &'static str,
    pub show: &'static str,
    pub record: &'static str,
    pub todo_add: &'static str,
    pub todo_update: &'static str,
    pub zoom: &'static str,
}
