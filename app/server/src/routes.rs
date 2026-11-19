//! The routes that read a port only the box holds: the secretary's briefings and the owner's
//! calendars. The routes the tab shares are `suzatary_api::routes`.

use chrono::NaiveDate;
use suzatary_api::wire;
use suzatary_base::escape::Escaped;
use suzatary_base::refusal::{Answered, Refusal};
use suzatary_google::calendar::page;
use suzatary_lines::secretary::conversations;
use suzatary_secretary::secretary;
use suzatary_secretary::secretary::briefings::Briefings;
use suzatary_todo::render::Render;
use suzatary_topics::secretary::answers::Answers;

/// Every day the assistant has saved a briefing for, capped.
const MAX_BRIEFINGS: usize = 400;

/// The saved briefings' days, newest first.
pub fn briefings(
    briefings: &dyn Briefings,
    conversations: conversations::Conversations<'_>,
) -> Answered<wire::BriefingsPayload> {
    let saved = briefings
        .all()
        .map_err(|e| Refusal::internal(e.to_string()))?;
    let mut briefs: Vec<secretary::briefs::Brief> = conversations::list(conversations)?
        .iter()
        .flat_map(secretary::briefs::briefs)
        .collect();
    briefs.sort_by(|a, b| b.date.cmp(&a.date).then(b.turn.cmp(&a.turn)));
    Ok(wire::BriefingsPayload {
        briefs: briefs
            .into_iter()
            .take(MAX_BRIEFINGS)
            .map(|brief| wire::BriefRow {
                date: brief.date.to_string(),
                conversation: brief.conversation.as_str().to_owned(),
                turn: u32::try_from(brief.turn).unwrap_or(u32::MAX),
                title: secretary::briefs::headline(&brief.text),
                words: u32::try_from(secretary::briefs::words(&brief.text)).unwrap_or(u32::MAX),
                text: brief.text,
            })
            .collect(),
        briefings: saved
            .into_iter()
            .take(MAX_BRIEFINGS)
            .map(|(day, source)| wire::BriefingRow {
                title: briefing_title(&source, day),
                words: u32::try_from(source.split_whitespace().count()).unwrap_or(u32::MAX),
                date: day.to_string(),
            })
            .collect(),
    })
}

/// The first `= …` heading's text, else the date, without a compile.
fn briefing_title(source: &str, day: NaiveDate) -> String {
    source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('='))
        .map(|line| line.trim_start_matches('=').trim())
        .find(|text| !text.is_empty())
        .map_or_else(|| day.to_string(), str::to_owned)
}

/// One day's briefing: the secretary's brief by block, else the saved file.
pub fn briefing(
    briefings: &dyn Briefings,
    render: &dyn Render,
    conversations: conversations::Conversations<'_>,
    answers: &dyn Answers,
    day: NaiveDate,
) -> Answered<wire::BriefingPayload> {
    let brief = conversations::list(conversations)?
        .iter()
        .flat_map(secretary::briefs::briefs)
        .filter(|brief| brief.date == day)
        .max_by_key(|brief| brief.turn);
    if let Some(brief) = brief {
        return Ok(wire::BriefingPayload {
            date: day.to_string(),
            day: wire::BriefingDay::Brief {
                conversation: brief.conversation.as_str().to_owned(),
                turn: u32::try_from(brief.turn).unwrap_or(u32::MAX),
                blocks: suzatary_topics::secretary::readings::blocks_of(answers, &brief.text),
                text: brief.text,
            },
        });
    }
    let source = briefings
        .read(day)
        .map_err(|_| Refusal::not_found(format!("no briefing for {day}")))?;
    let markup = Escaped::from_chain(source.clone());
    Ok(wire::BriefingPayload {
        date: day.to_string(),
        day: wire::BriefingDay::Saved {
            rich: render
                .html(&markup)
                .ok()
                .map(|html| html.as_str().to_owned()),
            source,
        },
    })
}

/// `GET /api/calendar/<date>`: a week of the owner's calendars, as Typst's HTML and plain text.
pub fn calendar(
    calendar: &dyn suzatary_google::calendar::effect::Calendar,
    render: &dyn Render,
    from: NaiveDate,
) -> Answered<wire::CalendarPayload> {
    const DAYS: u32 = 7;
    let days = calendar
        .days(from, DAYS)
        .map_err(|failed| Refusal::internal(failed.to_string()))?;
    Ok(wire::CalendarPayload {
        from: from.to_string(),
        days: DAYS,
        text: page::calendar_text(&days),
        rich: suzatary_todo::rendering::CALENDAR
            .call(&page::calendar(&days))
            .ok()
            .and_then(|page| render.page(&page).ok())
            .map(|html| html.as_str().to_owned()),
    })
}
