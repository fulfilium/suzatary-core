//! The fields a todo's forms are made of; what is typed is read at the form's edge, once.

use chrono::{Duration, NaiveDate};
use suzatary_record::todo::{Cycle, Deadline, Price};
use viewtree::Attr::{
    AriaLabel, AriaPressed, Class, Hidden, Max, Min, Placeholder, Role, Step, Type, Value,
};
use viewtree::{Node, view};

use crate::chrome::{pct, pie};

/// A one-line field.
pub(crate) fn line<M: 'static>(
    class: &str,
    label: &str,
    hint: &str,
    v: &str,
    typed: fn(String) -> M,
) -> Node<M> {
    view! { Input {Type => "text", Class => class.to_owned(), AriaLabel => label.to_owned(), Placeholder => (!hint.is_empty()).then(|| hint.to_owned()), Value => v.to_owned()} @input(typed) }
}

/// A day's field, no earlier than `min` where one is given.
pub(crate) fn day<M: 'static>(
    label: &str,
    min: Option<NaiveDate>,
    v: &str,
    typed: fn(String) -> M,
) -> Node<M> {
    view! { Input {Type => "date", Class => "draftfield", AriaLabel => label.to_owned(), Min => min.map(|d| d.to_string()), Value => v.to_owned()} @input(typed) }
}

/// A choice among chips: tapping the lit one clears it.
pub(crate) fn chips<M: Clone>(
    prompt: &str,
    options: &[String],
    v: &str,
    chose: impl Fn(String) -> M,
) -> Node<M> {
    let class = if v.is_empty() {
        "chipfield unchosen"
    } else {
        "chipfield"
    };
    let chip = |o: &String| {
        let on = o == v;
        let next = if on { String::new() } else { o.clone() };
        view! { Button {Type => "button", Class => if on { "chipbtn on" } else { "chipbtn" }, AriaPressed => if on { "true" } else { "false" }} @click(chose(next)) [o.as_str()] }
    };
    view! { Div {Class => class} [
        view! { P "chipprompt" {Hidden => !v.is_empty()} [prompt] },
        view! { Div "chiprow" {Role => "group", AriaLabel => prompt.to_owned()} [options.iter().map(chip).collect::<Vec<_>>()] },
    ] }
}

/// A field of Typst source, drawn by the host's editor; a new opening is a new editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Markup {
    /// What is typed.
    pub(crate) text: String,
    opening: String,
    openings: u32,
}

impl Markup {
    /// A field opened on `text`.
    #[must_use]
    pub(crate) fn of(text: String) -> Markup {
        Markup {
            opening: text.clone(),
            text,
            openings: 0,
        }
    }

    /// The field opened again on `text`, whatever was typed.
    pub(crate) fn reset(&mut self, text: String) {
        self.opening.clone_from(&text);
        self.text = text;
        self.openings += 1;
    }

    /// The field, `name` naming it among the page's.
    pub(crate) fn view<M: 'static>(
        &self,
        name: &str,
        class: &str,
        label: &str,
        rows: u32,
        typed: fn(String) -> M,
    ) -> Node<M> {
        let p = crate::part::Part::Editor {
            field: name.to_owned(),
            class: class.to_owned(),
            label: label.to_owned(),
            rows,
            text: self.opening.clone(),
        };
        view! { Div "markupfield" @input(typed) [crate::part::part(format!("{name}/{}", self.openings), &p)] }
    }
}

/// How a todo is priced: only its own kind's fields, as typed, read when the form is sent.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Pricing {
    /// A fulfillment that does not move, as a ratio.
    Flat(f64),
    /// A decay toward a day: the day and the lead-up in days, as typed.
    Deadline { day: String, lead: String },
    /// A cycle restarting at each pass.
    Recur(Cycling),
    /// A cycle on the calendar.
    Periodic(Cycling),
}

/// A cycle as typed: how many days it runs, and its two ends as ratios.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Cycling {
    pub(crate) days: String,
    /// Just after a pass.
    pub(crate) from: f64,
    /// Where it settles.
    pub(crate) to: f64,
}

/// A move of the price picker.
#[derive(Debug, Clone)]
pub enum PriceMsg {
    Kind(Kind),
    Flat(String),
    Day(String),
    Lead(String),
    Days(String),
    From(String),
    To(String),
}

/// The four kinds, as the picker names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Flat,
    Deadline,
    Recur,
    Periodic,
}

const KINDS: [(Kind, &str); 4] = [
    (Kind::Flat, "Flat"),
    (Kind::Deadline, "Deadline"),
    (Kind::Recur, "After a pass"),
    (Kind::Periodic, "On the calendar"),
];

/// The middle of the scale: a real number the author moves, never one that looks unset.
const DEFAULT_FLAT: f64 = 0.5;
const DEFAULT_LEAD: &str = "14";
const DEADLINE_START: u32 = 60;
const DEADLINE_END: u32 = 5;

fn default_cycle() -> Cycling {
    Cycling {
        days: "30".to_owned(),
        from: 0.7,
        to: 0.3,
    }
}

fn ratio(typed: &str) -> Option<f64> {
    typed
        .parse::<f64>()
        .ok()
        .map(|p| (p / 100.0).clamp(0.0, 1.0))
}

/// A ratio as the whole percent the box takes.
fn whole(r: f64) -> u32 {
    // In [0, 100] by the clamp.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let p = (r.clamp(0.0, 1.0) * 100.0).round() as u32;
    p
}

impl Default for Pricing {
    fn default() -> Self {
        Pricing::Flat(DEFAULT_FLAT)
    }
}

impl Pricing {
    /// A recurring price opens with its cycle, any other at the middle of the scale.
    #[must_use]
    pub(crate) fn of(cycle: Option<(Kind, Cycle)>) -> Pricing {
        let typed = |c: Cycle| Cycling {
            days: c.days.to_string(),
            from: f64::from(c.from_percent) / 100.0,
            to: f64::from(c.to_percent) / 100.0,
        };
        match cycle {
            Some((Kind::Recur, c)) => Pricing::Recur(typed(c)),
            Some((Kind::Periodic, c)) => Pricing::Periodic(typed(c)),
            Some((Kind::Flat | Kind::Deadline, _)) | None => Pricing::default(),
        }
    }

    fn kind(&self) -> Kind {
        match self {
            Pricing::Flat(_) => Kind::Flat,
            Pricing::Deadline { .. } => Kind::Deadline,
            Pricing::Recur(_) => Kind::Recur,
            Pricing::Periodic(_) => Kind::Periodic,
        }
    }

    fn cycle(&self) -> Cycling {
        match self {
            Pricing::Recur(c) | Pricing::Periodic(c) => c.clone(),
            Pricing::Flat(_) | Pricing::Deadline { .. } => default_cycle(),
        }
    }

    /// One move.
    pub(crate) fn update(&mut self, m: PriceMsg) {
        let cycled = |p: &Pricing, f: &dyn Fn(&mut Cycling)| {
            let mut c = p.cycle();
            f(&mut c);
            match p {
                Pricing::Periodic(_) => Pricing::Periodic(c),
                Pricing::Recur(_) | Pricing::Flat(_) | Pricing::Deadline { .. } => {
                    Pricing::Recur(c)
                }
            }
        };
        let next = match (m, &*self) {
            (PriceMsg::Kind(k), p) if k == p.kind() => return,
            // The two cycling kinds share a cycle; the rest start afresh.
            (PriceMsg::Kind(Kind::Flat), _) => Pricing::default(),
            (PriceMsg::Kind(Kind::Deadline), _) => Pricing::Deadline {
                day: String::new(),
                lead: DEFAULT_LEAD.to_owned(),
            },
            (PriceMsg::Kind(Kind::Recur), p) => Pricing::Recur(p.cycle()),
            (PriceMsg::Kind(Kind::Periodic), p) => Pricing::Periodic(p.cycle()),
            (PriceMsg::Flat(v), Pricing::Flat(_)) => {
                Pricing::Flat(ratio(&v).unwrap_or(DEFAULT_FLAT))
            }
            (PriceMsg::Day(day), Pricing::Deadline { lead, .. }) => Pricing::Deadline {
                day,
                lead: lead.clone(),
            },
            (PriceMsg::Lead(lead), Pricing::Deadline { day, .. }) => Pricing::Deadline {
                day: day.clone(),
                lead,
            },
            (PriceMsg::Days(days), p @ (Pricing::Recur(_) | Pricing::Periodic(_))) => {
                cycled(p, &|c| c.days.clone_from(&days))
            }
            (PriceMsg::From(v), p @ (Pricing::Recur(_) | Pricing::Periodic(_))) => {
                cycled(p, &|c| c.from = ratio(&v).unwrap_or(c.from))
            }
            (PriceMsg::To(v), p @ (Pricing::Recur(_) | Pricing::Periodic(_))) => {
                cycled(p, &|c| c.to = ratio(&v).unwrap_or(c.to))
            }
            // A field of a kind not shown moves nothing.
            (
                PriceMsg::Flat(_)
                | PriceMsg::Day(_)
                | PriceMsg::Lead(_)
                | PriceMsg::Days(_)
                | PriceMsg::From(_)
                | PriceMsg::To(_),
                _,
            ) => return,
        };
        *self = next;
    }

    /// The price the box takes, or what to say beside the picker.
    ///
    /// # Errors
    /// A deadline with no day or a lead-up out of range, or a cycle's days out of range.
    pub(crate) fn read(&self) -> Result<Price, String> {
        match self {
            Pricing::Flat(f) => Ok(Price::Flat { percent: whole(*f) }),
            Pricing::Deadline { day, lead } => {
                let end_date = NaiveDate::parse_from_str(day, "%Y-%m-%d")
                    .map_err(|_| "Pick a deadline date.".to_owned())?;
                let lead = lead
                    .parse::<i64>()
                    .ok()
                    .filter(|d| (1..=365).contains(d))
                    .ok_or_else(|| "The lead-up is a whole number of days, 1 to 365.".to_owned())?;
                Ok(Price::Deadline(Deadline {
                    end_date,
                    start_percent: DEADLINE_START,
                    end_percent: DEADLINE_END,
                    lead_up: Some(Duration::days(lead)),
                    start_date: None,
                }))
            }
            Pricing::Recur(c) | Pricing::Periodic(c) => {
                let days = c
                    .days
                    .parse::<u32>()
                    .ok()
                    .filter(|d| (1..=3650).contains(d))
                    .ok_or_else(|| "The cycle runs 1 to 3650 days.".to_owned())?;
                let cycle = Cycle {
                    days,
                    from_percent: whole(c.from),
                    to_percent: whole(c.to),
                };
                Ok(match self {
                    Pricing::Periodic(_) => Price::Periodic(cycle),
                    Pricing::Recur(_) | Pricing::Flat(_) | Pricing::Deadline { .. } => {
                        Price::Recur(cycle)
                    }
                })
            }
        }
    }

    /// `today` is the earliest day a deadline's field offers.
    #[must_use]
    pub(crate) fn view(&self, today: NaiveDate) -> Node<PriceMsg> {
        let kind = self.kind();
        let kinds = KINDS.map(|(k, label)| {
            let on = k == kind;
            view! { Button {Type => "button", Class => if on { "segment on" } else { "segment" }, AriaPressed => if on { "true" } else { "false" }} @click(PriceMsg::Kind(k)) [label] }
        });
        let range = |label: &str, v: f64, f: fn(String) -> PriceMsg| {
            view! { Input {Type => "range", Class => "draftrange", Min => "0", Max => "100", Step => "1", AriaLabel => label.to_owned(), Value => whole(v).to_string()} @input(f) }
        };
        let share = |v: f64| view! { Span "draftpct" [pie(v), pct(v)] };
        let meta = |s: &str| view! { Span "meta" [s] };
        let days = |label: &str, max: &str, v: &str, f: fn(String) -> PriceMsg| {
            view! { Input {Type => "number", Class => "draftfield draftdays", Min => "1", Max => max.to_owned(), Step => "1", AriaLabel => label.to_owned(), Value => v.to_owned()} @input(f) }
        };
        let rows = match self {
            Pricing::Flat(f) => view! { Div "draftrow" [
                range("fulfillment, as a percentage — low means urgent", *f, PriceMsg::Flat),
                share(*f),
                meta("low means urgent"),
            ] },
            Pricing::Deadline { day, lead } => view! { Div "draftrow" [
                view! { Input {Type => "date", Class => "draftfield", Min => today.to_string(), AriaLabel => "deadline date", Value => day.clone()} @input(PriceMsg::Day) },
                days("lead-up, in days", "365", lead, PriceMsg::Lead),
                meta("days of lead-up"),
            ] },
            Pricing::Recur(c) | Pricing::Periodic(c) => view! { Div "cyclefields" [
                view! { Div "draftrow" [days("how many days the cycle runs", "3650", &c.days, PriceMsg::Days), meta("days")] },
                view! { Div "draftrow" [range("fulfillment just after a pass", c.from, PriceMsg::From), share(c.from), meta("just after a pass")] },
                view! { Div "draftrow" [range("fulfillment the cycle settles at", c.to, PriceMsg::To), share(c.to), meta("where it settles")] },
            ] },
        };
        view! { Div "pricefields" [
            view! { Div "segmented" {Role => "group", AriaLabel => "how this todo is priced"} [kinds.to_vec()] },
            rows,
        ] }
    }
}

/// What a form says beside its button: nothing, or why what was sent was refused.
#[must_use]
pub(crate) fn refusal<M>(said: Option<&str>) -> Node<M> {
    view! { P "formrefusal" {Role => "alert", Hidden => said.is_none()} [said.unwrap_or_default()] }
}
