//! One sync, as a function: the cursor, the calendars it may read, the time and what each feed
//! answered, to the items learned and the next cursor.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Duration, NaiveDateTime, NaiveTime};
use icalendar::parser::{read_components, unfold};
use serde::{Deserialize, Serialize};

use crate::event;
use crate::item::{Item, Status, Window};

/// What one feed's request came back as: a status and a body, or the host's refusal.
pub type Fetched = Result<(u16, Vec<u8>), String>;

/// What a sync learned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Learned {
    /// Every occurrence new or changed since the cursor, and every one gone from its feed while
    /// still in the window, as cancelled.
    pub items: Vec<Item>,
    /// Where the next sync starts.
    pub cursor: Vec<u8>,
}

/// What the last sync held of each calendar: the occurrences in its window, as emitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    version: u32,
    calendars: BTreeMap<String, Vec<Item>>,
}

/// Read every calendar in `calendars` from `cursor`, in a window from 7 days before today, so
/// what a meeting left owed stays in view, to 60 after, a day of slack each side.
///
/// A cursor this plugin cannot read is read as none. A feed that cannot be read leaves its
/// calendar as the cursor had it. A calendar the cursor holds and `calendars` no longer names
/// is read as empty, so its occurrences in the window are learned cancelled once.
///
/// # Panics
/// Never: a cursor is strings and lists, which JSON always prints.
pub fn sync(
    cursor: Option<&[u8]>,
    calendars: &[String],
    now: NaiveDateTime,
    fetch: &mut dyn FnMut(&str) -> Fetched,
    log: &mut dyn FnMut(String),
) -> Learned {
    let mut held = match cursor.map(serde_json::from_slice::<Cursor>) {
        Some(Ok(held)) if held.version == 1 => held.calendars,
        Some(Ok(_) | Err(_)) => {
            log("the cursor is not this plugin's: starting afresh".to_owned());
            BTreeMap::new()
        }
        None => BTreeMap::new(),
    };
    let midnight = now.date().and_time(NaiveTime::MIN);
    let window = (midnight - Duration::days(8), midnight + Duration::days(61));
    let every: BTreeSet<String> = held.keys().chain(calendars).cloned().collect();
    let (mut items, mut kept) = (Vec::new(), BTreeMap::new());
    for name in every {
        let before = held.remove(&name).unwrap_or_default();
        let named = calendars.contains(&name);
        let mut said = |line: String| log(format!("{name}: {line}"));
        let current = named.then(|| read_feed(&name, fetch(&name), window, &mut said));
        match current.unwrap_or(Ok(Vec::new())) {
            Ok(current) => {
                let (learned, now_held) = changes(&before, current, window);
                let (count, new) = (now_held.len(), learned.len());
                log(format!("{name}: {count} in the window, {new} learned"));
                items.extend(learned);
                if named {
                    kept.insert(name, now_held);
                }
            }
            Err(why) => {
                log(format!("{name}: {why}"));
                kept.insert(name, before);
            }
        }
    }
    let cursor = serde_json::to_vec(&Cursor {
        version: 1,
        calendars: kept,
    });
    let cursor = cursor.expect("a cursor is plain data");
    Learned { items, cursor }
}

/// The feed's occurrences in the window, or why it could not be read.
fn read_feed(
    name: &str,
    fetched: Fetched,
    window: Window,
    said: event::Said<'_>,
) -> Result<Vec<Item>, String> {
    let body = match fetched.map_err(|why| format!("not fetched: {why}"))? {
        (200, body) => String::from_utf8(body).map_err(|_| "the feed is not UTF-8".to_owned())?,
        (status, _) => return Err(format!("the feed answered {status}")),
    };
    let text = unfold(body.strip_prefix('\u{feff}').unwrap_or(&body));
    // A component's name is case-insensitive, and the parser reads BEGIN and END in capitals and
    // one top-level component at a time, so the text is cut into one piece per VCALENDAR.
    let mut pieces: Vec<String> = Vec::new();
    for line in text.split_inclusive('\n') {
        let upper = line.to_ascii_uppercase();
        let keyword = upper.starts_with("BEGIN:") || upper.starts_with("END:");
        let line = if keyword { upper } else { line.to_owned() };
        if pieces.is_empty() || line.trim_end() == "BEGIN:VCALENDAR" {
            pieces.push(String::new());
        }
        if let Some(piece) = pieces.last_mut() {
            piece.push_str(&line);
        }
    }
    let mut calendars = Vec::new();
    for piece in &pieces {
        let roots = read_components(piece).map_err(|_| "not iCalendar".to_owned())?;
        calendars.extend(
            roots
                .into_iter()
                .filter(|root| root.name.as_str() == "VCALENDAR"),
        );
    }
    let mut calendars = calendars.into_iter();
    let first = calendars.next().ok_or("not iCalendar")?;
    let calendar = calendars.fold(first, |mut all, one| {
        all.properties.extend(one.properties);
        all.components.extend(one.components);
        all
    });
    Ok(event::occurrences(&calendar, name, window, said))
}

/// The items to emit and what the cursor now holds: new and changed occurrences, and once, as
/// cancelled, each one the cursor held that the feed no longer has while it is in the window.
fn changes(before: &[Item], current: Vec<Item>, window: Window) -> (Vec<Item>, Vec<Item>) {
    let key = |item: &Item| (item.uid.clone(), item.occurrence.clone());
    let was: BTreeMap<_, &Item> = before.iter().map(|item| (key(item), item)).collect();
    let now: BTreeSet<_> = current.iter().map(key).collect();
    let changed = |item: &&Item| was.get(&key(item)) != Some(item);
    let mut learned: Vec<Item> = current.iter().filter(changed).cloned().collect();
    let mut kept = current;
    for item in before.iter().filter(|item| !now.contains(&key(item))) {
        if item.overlaps(window) {
            let status = Status::Cancelled;
            let gone = Item {
                status,
                ..item.clone()
            };
            learned.extend((item.status != status).then(|| gone.clone()));
            kept.push(gone);
        }
    }
    kept.sort_by_key(key);
    (learned, kept)
}
