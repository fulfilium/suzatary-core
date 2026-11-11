//! A changeset in one line: its acts grouped in the order they first appear, a todo named when it
//! is one and counted when several, and flags for what the owner must see to decide.

use suzatary_core::wire::inbox::{ObjectAct, ProposedObject};

/// SECRETARY.md's review holds a todo priced at or under this for the owner.
pub(crate) const HOLD: u32 = 45;

#[must_use]
pub(crate) fn phrase(act: &ObjectAct, what: &str) -> String {
    let verb = match act {
        ObjectAct::Added { .. } => "add",
        ObjectAct::Edited { .. } => "reword",
        ObjectAct::Repriced { .. } | ObjectAct::Respecified { .. } => "reprice",
        ObjectAct::Completed { .. } => "complete",
        ObjectAct::Cancelled { .. } => "cancel",
        ObjectAct::Reopened { .. } => "reopen",
        ObjectAct::Tended { .. } => return format!("record a pass at {what}"),
        ObjectAct::Created { .. } => "register",
    };
    format!("{verb} {what}")
}

#[must_use]
pub(crate) fn sentence(o: &ProposedObject) -> String {
    phrase(&o.act, &o.act.todo().todo)
}

/// The box prints a price `{:.0}%` first, then any recurrence; anything else has no number.
fn percent(printed: &str) -> Option<u32> {
    let digits: String = printed.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || !printed[digits.len()..].starts_with('%') {
        return None;
    }
    digits.parse().ok()
}

struct Group<'a> {
    act: &'a ObjectAct,
    todos: Vec<&'a str>,
}

/// The line, then its flags: the todos priced at or under `HOLD`, and those not on the chain yet.
#[must_use]
pub(crate) fn lines(rows: &[ProposedObject]) -> (String, Vec<String>) {
    let mut groups: Vec<Group<'_>> = Vec::new();
    let mut urgent: Vec<(&str, u32)> = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    for o in rows {
        let todo = o.act.todo().todo.as_str();
        match groups.iter_mut().find(|g| g.act.word() == o.act.word()) {
            Some(g) if g.todos.contains(&todo) => {}
            Some(g) => g.todos.push(todo),
            None => groups.push(Group {
                act: &o.act,
                todos: vec![todo],
            }),
        }
        let price = o
            .act
            .changes()
            .iter()
            .find(|c| c.field == "priority")
            .and_then(|c| percent(&c.after));
        if let Some(p) = price.filter(|p| *p <= HOLD)
            && !urgent.contains(&(todo, p))
        {
            urgent.push((todo, p));
        }
        if o.act.todo().pending_todo && !pending.contains(&todo) {
            pending.push(todo);
        }
    }
    let line = groups
        .iter()
        .map(|g| match g.todos.as_slice() {
            [one] => phrase(g.act, one),
            many => phrase(g.act, &format!("{} todos", many.len())),
        })
        .collect::<Vec<_>>()
        .join(" · ");
    let mut flags = Vec::new();
    if !urgent.is_empty() {
        let priced: Vec<String> = urgent.iter().map(|(t, p)| format!("{t} at {p}%")).collect();
        flags.push(format!(
            "At or under the {HOLD}% the review holds: {}",
            priced.join(", ")
        ));
    }
    if !pending.is_empty() {
        flags.push(format!(
            "Not on the chain yet, waiting on another card: {}",
            pending.join(", ")
        ));
    }
    (line, flags)
}
