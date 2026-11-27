//! THE BURST LAW: a burst of compactions over one log reads the log from the provider's cache on
//! every call after the first, and a call no other call shares is sent with no cache mark at all.
//!
//! The provider is the fake of `cache.rs`; the CLI's marks are the ones it captured there: two on
//! the system text (one here, which stands for both) and one on the message's last block, and none
//! when the call turns caching off.

use std::sync::Mutex;

use crate::cache::{Block, Mark, Part, Provider, Running, Served};
use suzatary_base::capability::Nothing;
use suzatary_conversations::secretary::model::{ModelError, Plain};
use suzatary_digest::model_name::ModelName;
use suzatary_digest::secretary::compact::Compact;
use suzatary_digest::secretary::compact_ask::{Ask, Sharing};
use suzatary_digest::secretary::compact_budget::Budget;
use suzatary_digest::secretary::compaction::{Compaction, Paced, Performed, Round};
use suzatary_held::for_model::ForModel;
use suzatary_io_model::model::stdin_after;
use suzatary_secretary::secretary::live_shape::{Shape, Summaries};

/// How a call's text reaches the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// One block, as every call was sent before: the CLI marks it, so the whole call is written.
    Whole,
    /// What [`suzatary_io_model::model::ClaudeCli::plain_after`] sends.
    Split,
}

/// A compactor that answers one line and keeps what the provider served each call, with the
/// log each call opened with.
struct Recording {
    name: ModelName,
    layout: Layout,
    provider: Mutex<Provider>,
    calls: Mutex<Vec<(String, Served)>>,
}

impl Recording {
    fn new(layout: Layout) -> Self {
        Self {
            name: ModelName::parse("a-model").expect("a model"),
            layout,
            provider: Mutex::default(),
            calls: Mutex::default(),
        }
    }

    /// `ask` as the provider reads it.
    fn blocks(&self, ask: &Ask) -> Vec<Block> {
        let system = |mark| Block {
            text: ask.prompt().to_owned(),
            part: Part::System,
            mark,
        };
        let (opening, own) = (ask.opening(), ask.own());
        match (self.layout, ask.sharing()) {
            (Layout::Whole, _) => vec![
                system(Mark::Cli),
                Block {
                    text: opening + &own,
                    part: Part::Tail,
                    mark: Mark::Cli,
                },
            ],
            (Layout::Split, Sharing::Alone) => vec![
                system(Mark::None),
                Block {
                    text: opening + &own,
                    part: Part::Tail,
                    mark: Mark::None,
                },
            ],
            (Layout::Split, Sharing::Shared) => {
                let stdin = stdin_after(&ForModel::of(&opening), &ForModel::of(&own));
                let message: serde_json::Value =
                    serde_json::from_str(stdin.payload.trim()).expect("one JSON message");
                let content = message["message"]["content"].as_array().expect("blocks");
                let mut blocks = vec![system(Mark::Cli)];
                blocks.extend(content.iter().map(|block| Block {
                    text: block["text"].as_str().expect("text").to_owned(),
                    part: Part::Record,
                    mark: if block.get("cache_control").is_some() {
                        Mark::Ours
                    } else {
                        Mark::None
                    },
                }));
                if let Some(last) = blocks.last_mut() {
                    last.part = Part::Tail;
                    last.mark = Mark::Cli;
                }
                blocks
            }
        }
    }
}

impl Compact for Recording {
    fn model(&self) -> &ModelName {
        &self.name
    }

    fn compact(&self, ask: &Ask) -> Result<Plain, ModelError> {
        let mut calls = self.calls.lock().expect("held");
        let served =
            self.provider
                .lock()
                .expect("held")
                .serve(&self.blocks(ask), calls.len() * 10, None);
        calls.push((ask.opening(), served));
        Ok(Plain {
            text: "The owner asked about the permit.".to_owned(),
            usage: None,
        })
    }
}

/// A burst after a restart: a store of lines with nothing summarised, compacted one claim at a
/// time until nothing is owed. What the provider served each call, in order.
fn burst(layout: Layout) -> Vec<(String, Served)> {
    let running = Running::of(Shape {
        lines: 600,
        conversations: 3,
        answer_bytes: 600,
        summaries: Summaries::First(0),
    });
    let memories = running.memories();
    let compactor = Recording::new(layout);
    let rung: &dyn Compact = &compactor;
    let paced = Paced::new(
        Compaction {
            ladder: suzatary_conversations::nonempty::NonEmpty::new(rung, Vec::new()),
        },
        Budget {
            calls_per_hour: u32::MAX,
            bytes_per_day: u64::MAX,
        },
    );
    while let Round::Taken(claims) = paced.take(&memories, 1, &Nothing).expect("a round") {
        for claim in claims {
            let done = claim.perform(&memories, &Nothing, &Nothing);
            assert!(matches!(done, Ok(Performed::Compacted(_))), "{done:?}");
        }
    }
    compactor.calls.into_inner().expect("held")
}

fn share(calls: &[&Served]) -> usize {
    let read: usize = calls.iter().map(|call| call.read).sum();
    let all: usize = calls
        .iter()
        .map(|call| call.read + call.written + call.fresh)
        .sum();
    read * 100 / all.max(1)
}

/// Calls that open with the same log, by that log, in order.
fn by_log(calls: &[(String, Served)]) -> Vec<Vec<&Served>> {
    let mut logs: Vec<(&str, Vec<&Served>)> = Vec::new();
    for (log, served) in calls {
        match logs.iter_mut().find(|(seen, _)| seen == log) {
            Some((_, group)) => group.push(served),
            None => logs.push((log, vec![served])),
        }
    }
    logs.into_iter().map(|(_, group)| group).collect()
}

/// The first call that opens with a log writes it, each later one reads it, and a log only one
/// call opens with is never marked, so that call writes nothing and reads nothing.
#[test]
fn a_burst_reads_its_log_after_the_first_call_and_a_lone_call_marks_nothing() {
    let calls = burst(Layout::Split);
    let groups = by_log(&calls);
    assert!(
        groups.iter().any(|group| group.len() > 2),
        "a burst of many calls over one log"
    );
    for group in &groups {
        let (first, later) = group.split_first().expect("a call");
        if later.is_empty() {
            assert_eq!((first.read, first.written), (0, 0), "a lone call");
            continue;
        }
        assert!(first.written > 0, "the first call writes the log");
        for call in later {
            assert!(
                call.read > call.written,
                "a later call reads the log the first wrote: {call:?} after {first:?}"
            );
        }
    }
}

#[test]
#[ignore = "a measurement, printed: cargo test --release -p suzatary-secretary --test all compact_cache -- --ignored --nocapture"]
fn the_read_share_of_a_burst_before_and_after() {
    for layout in [Layout::Whole, Layout::Split] {
        let calls = burst(layout);
        let served: Vec<&Served> = calls.iter().map(|(_, served)| served).collect();
        let (read, written, fresh) = served.iter().fold((0, 0, 0), |(r, w, f), call| {
            (r + call.read, w + call.written, f + call.fresh)
        });
        eprintln!(
            "{layout:?}: {} calls, {}% read ({} bytes read, {written} written, {fresh} fresh)",
            served.len(),
            share(&served),
            read
        );
    }
}
