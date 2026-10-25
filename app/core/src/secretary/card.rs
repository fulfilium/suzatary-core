//! A proposal's card, derived once: every word and answer is decided here, so the web app draws
//! any card with one function and never asks which kind it is.

use crate::coding::RoutineId;
use crate::repos::Visibility;
use crate::secretary::inbox_reading::Reading;
use crate::secretary::store::Style;
use crate::web::WebKind;
use crate::wire::{
    Asker, Asks, Cached, Card, CardAnswer, CardField, CardFile, CardLink, HeldIn, ProposedAct,
    RememberWords,
};

/// The card for a proposal.
#[must_use]
#[expect(clippy::too_many_lines, reason = "one arm per kind of act")]
pub fn of(act: &ProposedAct) -> Card {
    match act {
        ProposedAct::Email {
            account,
            to,
            cc,
            subject,
            body,
            attachments,
            held,
        } => Card {
            files: attachments.clone(),
            ..email(account, (to, cc), subject, body, attachments.len(), held)
        },
        ProposedAct::Forward {
            account,
            to,
            cc,
            subject,
            note,
            attachments,
            original,
            style,
            held,
        } => forward(
            email(
                account,
                (to, cc),
                subject,
                note,
                attachments.len(),
                &HeldIn::Read { values: Vec::new() },
            ),
            &to.join(", "),
            original,
            *style,
            held,
            attachments,
        ),
        ProposedAct::Changes {
            changes, problem, ..
        } => changeset(changes, problem),
        ProposedAct::Auth { account, sentence } => Card {
            answers: vec![CardAnswer {
                done: format!("running: authorising {account}"),
                asks: Asks::Confirm {
                    question: question(sentence),
                    yes: "Yes, run it".to_owned(),
                },
                ..answer("run", "Run")
            }],
            ..card(
                "Proposed act",
                sentence,
                vec![field("Account", account)],
                "run",
            )
        },
        ProposedAct::AuthSend { account, sentence } => auth_send(account, sentence),
        ProposedAct::Secret { label, found, link } => {
            secret(label, found.as_deref(), link.as_deref())
        }
        ProposedAct::Web {
            request,
            target,
            host,
            always,
            sentence,
        } => web(*request, target, host, always, sentence),
        ProposedAct::Patch {
            repo,
            visibility,
            base,
            message,
            files,
            patch: diff,
            sentence,
        } => patch(repo, *visibility, base, message, files, diff, sentence),
        ProposedAct::Drive {
            name,
            folder,
            size,
            sentence,
            ..
        } => document(name, folder, size, sentence),
        ProposedAct::Remember {
            words,
            about,
            sentence,
        } => remember(words, *about, sentence),
        ProposedAct::CodingSession { task, sentence } => coding_session(task, sentence),
        ProposedAct::Device {
            actor,
            key,
            sentence,
        } => device(actor, key, sentence),
        ProposedAct::Decision {
            title,
            body,
            rationale,
            supersedes,
            sentence,
            ..
        } => decision(title, body, rationale, supersedes, sentence),
        ProposedAct::SignIn { sentence } => sign_in(sentence),
        ProposedAct::SignInCode { url, sentence } => sign_in_code(url, sentence),
        ProposedAct::UnlockStore { sentence } => unlock_store(sentence),
        ProposedAct::Connect {
            connection,
            writes,
            steps,
            reach,
            sentence,
        } => connect(connection, writes, steps, reach, sentence),
        ProposedAct::Disconnect {
            connection,
            writes,
            forgets,
            cached,
            sentence,
        } => disconnect(connection, writes, forgets, *cached, sentence),
        ProposedAct::Grant {
            plugin,
            origin,
            sentence,
        } => grant(plugin, origin, sentence),
        ProposedAct::Erase { sentence } => erase(sentence),
        ProposedAct::GoogleClient {
            redirect, steps, ..
        } => google_client(redirect, steps),
        ProposedAct::GoogleSignIn {
            account,
            parts,
            consent,
            sentence,
        } => google_sign_in(account, parts, consent.as_deref(), sentence),
        ProposedAct::CalendarEvent {
            account,
            event,
            sentence,
        } => calendar_event(account, event, sentence),
    }
}

/// The box's Google Cloud client: every step to make it, numbered, with the address it must
/// list written into its step, and a field the downloaded file is pasted into whole.
fn google_client(redirect: &str, steps: &[String]) -> Card {
    let numbered: Vec<String> = steps
        .iter()
        .enumerate()
        .map(|(at, step)| {
            format!(
                "{}. {}",
                at + 1,
                step.replace(
                    "the address this card shows",
                    &format!("the address {redirect}")
                )
            )
        })
        .collect();
    Card {
        answers: vec![CardAnswer {
            done: "kept the Google client".to_owned(),
            asks: Asks::Value {
                input: "The client's downloaded file (JSON)".to_owned(),
            },
            ..answer("store", "Keep it")
        }],
        link: Some(CardLink {
            label: "Open the Google Cloud console".to_owned(),
            href: crate::google::CONSOLE.to_owned(),
        }),
        ..card(
            "Make the box's Google client",
            &format!(
                "Every Google account signs in through one Google Cloud client of your own, made \
                 once. It takes about five minutes:\n{}",
                numbered.join("\n")
            ),
            vec![field("Authorized redirect URI", redirect)],
            "store",
        )
    }
}

/// A sign-in with Google: the tap opens a consent, whose page the card's button opens while it
/// is open.
fn google_sign_in(account: &str, parts: &[String], consent: Option<&str>, sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "the consent is open: press Continue to Google".to_owned(),
            asks: Asks::Confirm {
                question: format!(
                    "Sign {account} in with Google? The box opens Google's consent page; choose {account} there and allow every part it lists."
                ),
                yes: "Yes, sign in".to_owned(),
            },
            ..answer("sign_in", "Sign in with Google")
        }],
        link: consent.map(|href| CardLink {
            label: "Continue to Google".to_owned(),
            href: href.to_owned(),
        }),
        ..card(
            "Sign in with Google",
            sentence,
            vec![
                field("Account", account),
                field("It grants", &parts.join(", ")),
                field(
                    "If Google warns",
                    "\"Google hasn't verified this app\" is your own client: choose Advanced, \
                     then Go to the app.",
                ),
            ],
            "sign in",
        )
    }
}

/// An event to add to a Google account's own calendar.
fn calendar_event(account: &str, event: &str, sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "added to the calendar".to_owned(),
            asks: Asks::Confirm {
                question: format!("Add {event} to {account}'s calendar?"),
                yes: "Yes, add it".to_owned(),
            },
            ..answer("add", "Add to calendar")
        }],
        uncertain: "It may have been added. Check the calendar; it is never added again."
            .to_owned(),
        ..card(
            "Add a calendar event",
            sentence,
            vec![field("Calendar", account), field("Event", event)],
            "add",
        )
    }
}

/// A Microsoft account's consent to send: the device-code sign-in for the SMTP grant alone.
fn auth_send(account: &str, sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: format!("running: authorising {account} to send"),
            asks: Asks::Confirm {
                question: format!(
                    "Sign {account} in to send mail? The box shows a code to enter on Microsoft's page; reading is not touched."
                ),
                yes: "Yes, sign in to send".to_owned(),
            },
            ..answer("run", "Sign in to send")
        }],
        ..card(
            "Sign in to send",
            sentence,
            vec![field("Account", account)],
            "run",
        )
    }
}

/// A connection to make: what it writes, what it may reach and the steps that follow, each a
/// card of its own once the tap has written it.
fn connect(
    connection: &str,
    writes: &str,
    steps: &[String],
    reach: &[String],
    sentence: &str,
) -> Card {
    let fields = [
        Some(field("Connection", connection)),
        Some(field("It writes", writes)),
        (!reach.is_empty()).then(|| field("It may reach", &reach.join(", "))),
        (!steps.is_empty()).then(|| field("Then", &steps.join("; "))),
    ];
    Card {
        answers: vec![CardAnswer {
            done: "connected".to_owned(),
            asks: Asks::Confirm { question: "Connect it? The box writes it into the instance file, then files a card for each step." .to_owned(), yes: "Yes, connect it".to_owned() },
            ..answer("connect", "Connect")
        }],
        ..card(
            &format!("Connect {connection}"),
            sentence,
            fields.into_iter().flatten().collect(),
            "connect",
        )
    }
}

/// A connection to remove: what goes and what stays; a mail account's card also offers to
/// delete the mail kept as its cache, as its wider yes.
fn disconnect(
    connection: &str,
    writes: &str,
    forgets: &[String],
    cached: Cached,
    sentence: &str,
) -> Card {
    let remove = CardAnswer {
        done: "removed".to_owned(),
        asks: Asks::Confirm {
            question: "Remove it from the instance file?".to_owned(),
            yes: "Yes, remove it".to_owned(),
        },
        ..answer("remove", "Remove")
    };
    let (stays, answers) = match cached {
        Cached::Nothing => ("Its todos, cards and conversations.", vec![remove]),
        Cached::Mail => (
            "Its todos, cards and conversations. Its mail stays on the box unless you remove it \
             with its mail; a message no longer kept reads as no longer stored.",
            vec![
                CardAnswer {
                    label: "Remove, keep its mail".to_owned(),
                    asks: Asks::Confirm {
                        question: "Remove it, and keep its mail on the box?".to_owned(),
                        yes: "Yes, remove it".to_owned(),
                    },
                    ..remove
                },
                CardAnswer {
                    done: "removed, and its mail deleted".to_owned(),
                    asks: Asks::Always {
                        question: "Remove it, and delete the mail kept for it on the box?"
                            .to_owned(),
                        yes: "Yes, remove it and its mail".to_owned(),
                    },
                    ..answer("forget", "Remove and delete its mail")
                },
            ],
        ),
    };
    let fields = [
        Some(field("Connection", connection)),
        Some(field("It removes", writes)),
        (!forgets.is_empty()).then(|| field("It forgets", &forgets.join(", "))),
        Some(field("Stays", stays)),
    ];
    Card {
        answers,
        ..card(
            &format!("Remove {connection}"),
            sentence,
            fields.into_iter().flatten().collect(),
            "remove",
        )
    }
}

/// One more origin a plugin may reach, named alone: the link it was read from is never stored.
fn grant(plugin: &str, origin: &str, sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "allowed".to_owned(),
            asks: Asks::Confirm {
                question: format!(
                    "Let {} fetch from {origin}?",
                    crate::connect::plugin_called(plugin)
                ),
                yes: "Yes, allow it".to_owned(),
            },
            ..answer("allow", "Allow")
        }],
        ..card(
            &format!(
                "Let {} reach a new address",
                crate::connect::plugin_called(plugin)
            ),
            sentence,
            vec![
                field("Plugin", plugin),
                field("Address", origin),
                field(
                    "Then",
                    "Set the link again in its card: nothing was stored while the address was \
                     not allowed.",
                ),
            ],
            "allow",
        )
    }
}

/// The card at its reading: the yes a tap may still perform, none where no yes is safe.
///
/// After a failure that sent nothing it is the main answer again as Try again, never "Always
/// allow", whose rule was written before the act that failed.
#[must_use]
pub fn at(card: Card, reading: &Reading) -> Card {
    match reading {
        Reading::Standing => card,
        Reading::One(answer) if answer.response.performs() => Card {
            answers: card
                .answers
                .into_iter()
                .filter(|answer| !matches!(answer.asks, Asks::Always { .. }))
                .take(1)
                .map(|answer| CardAnswer {
                    label: "Try again".to_owned(),
                    ..answer
                })
                .collect(),
            ..card
        },
        Reading::One(_) | Reading::Conflict(_) | Reading::Unreadable(_) => Card {
            answers: Vec::new(),
            ..card
        },
    }
}

/// A coding session's card: the whole task, and Start saying what a start
/// spends and hands over.
fn coding_session(task: &str, sentence: &str) -> Card {
    Card {
        body: task.to_owned(),
        answers: vec![CardAnswer {
            done: "started".to_owned(),
            asks: Asks::Confirm { question: "Start a cloud coding session on this task? It spends the account's usage and hands the repository to an agent, which pushes a claude/ branch and a report; nothing is merged." .to_owned(), yes: "Yes, start it".to_owned() },
            ..answer("start", "Start")
        }],
        uncertain:
            "A session may have started. Check the routine's sessions; it is never started again."
                .to_owned(),
        ..card("Proposed coding session", sentence, Vec::new(), "start")
    }
}

/// A device's card: the whole key, and a yes that this device signs, so only a device already
/// trusted can say it.
fn device(actor: &str, key: &str, sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "trusted".to_owned(),
            asks: Asks::Endorse {
                question: format!(
                    "Trust this device to write as {actor}? Say yes only for a device you added yourself: what it signs will stand as yours."
                ),
                yes: "Yes, trust it".to_owned(),
            },
            ..answer("trust", "Trust")
        }],
        ..card(
            "A device asks to be trusted",
            sentence,
            vec![field("Writes as", actor), field("Key", key)],
            "trust",
        )
    }
}

/// A decision's card: every field whole, and Record saying what it supersedes.
fn decision(
    title: &str,
    body: &str,
    rationale: &str,
    supersedes: &[String],
    sentence: &str,
) -> Card {
    let mut fields = vec![field("Title", title), field("Why", rationale)];
    if !supersedes.is_empty() {
        fields.push(field("Supersedes", &supersedes.join(", ")));
    }
    Card {
        body: body.to_owned(),
        answers: vec![CardAnswer {
            done: "recorded".to_owned(),
            asks: Asks::Confirm {
                question: if supersedes.is_empty() {
                    "Record this decision as yours?".to_owned()
                } else {
                    format!(
                        "Record this decision as yours? It supersedes {}, which stop being in force.",
                        supersedes.join(", ")
                    )
                },
                yes: "Yes, record it".to_owned(),
            },
            ..answer("record", "Record")
        }],
        ..card("Record this decision?", sentence, fields, "record")
    }
}

/// A remember card: the fact whole, and Remember; once its words are deleted, or where they are
/// not read, what it was about and no answer, since there is nothing to show to remember.
fn remember(words: &RememberWords, about: crate::secretary::memory::Kind, sentence: &str) -> Card {
    let about = about.as_str();
    match words {
        RememberWords::Kept { fact } => Card {
            answers: vec![CardAnswer {
                done: "remembered".to_owned(),
                asks: Asks::Confirm {
                    question: format!(
                        "Remember this? Every conversation from now on reads it as background: {fact}"
                    ),
                    yes: "Yes, remember it".to_owned(),
                },
                ..answer("remember", "Remember")
            }],
            ..card(
                "Remember this?",
                sentence,
                vec![field("Kind", about), field("Fact", fact)],
                "remember",
            )
        },
        RememberWords::Gone {} => card(
            "Proposed remembering",
            sentence,
            vec![field("Kind", about), field("Fact", "deleted")],
            "remember",
        ),
        RememberWords::Unread {} => card(
            "Proposed remembering",
            sentence,
            vec![field("Kind", about), field("Fact", "not read here")],
            "remember",
        ),
    }
}

/// A patch's card: the whole diff, and Open saying when the repository is public, since its
/// branch is readable the moment it is pushed.
fn patch(
    repo: &str,
    visibility: Visibility,
    base: &str,
    message: &str,
    files: &[String],
    diff: &str,
    sentence: &str,
) -> Card {
    Card {
        body: diff.to_owned(),
        answers: vec![CardAnswer {
            done: format!("pull request opened on {repo}"),
            asks: Asks::Confirm {
                question: if visibility == Visibility::Public {
                    format!(
                        "Open a pull request on {repo}? It is public: anyone can read its branch once pushed."
                    )
                } else {
                    format!("Open a pull request on {repo}?")
                },
                yes: "Yes, open it".to_owned(),
            },
            ..answer("open", "Open pull request")
        }],
        uncertain: format!(
            "It may have been opened. Check {repo}'s pull requests; it is never opened again."
        ),
        ..card(
            "Proposed change",
            sentence,
            vec![
                field("Repository", &format!("{repo} ({visibility})")),
                field("Commit message", message),
                field("Files", &files.join(", ")),
                field("Against", &base.chars().take(12).collect::<String>()),
            ],
            "open",
        )
    }
}

/// A document request's card: the file, where it is, Allow once (this file, read now and
/// attachable from the copy kept here), and Always allow this folder.
fn document(name: &str, folder: &str, size: &str, sentence: &str) -> Card {
    let size = if size.is_empty() { "unknown" } else { size };
    Card {
        answers: vec![
            CardAnswer {
                done: format!("reading {name}"),
                asks: Asks::Confirm {
                    question: format!(
                        "Allow {name} once? It is read into this conversation, with any SSN, ITIN, TIN or EIN, code, or card or bank number replaced by [redacted], and the copy read can go on an email you tap."
                    ),
                    yes: "Yes, allow once".to_owned(),
                },
                ..answer("read", "Allow once")
            },
            CardAnswer {
                done: format!("allowed: {folder}"),
                asks: Asks::Always {
                    question: format!(
                        "Always allow {folder}? This file is read now, and any later file under that folder is read without asking."
                    ),
                    yes: "Yes, always".to_owned(),
                },
                ..answer("always", "Always allow this folder")
            },
        ],
        ..card(
            "Document request",
            sentence,
            vec![
                field("File name", name),
                field("Folder", folder),
                field("Size", size),
            ],
            "read",
        )
    }
}

/// A size in bytes, in words: "812 B", "48.2 KB", "3.1 MB".
#[must_use]
pub(crate) fn bytes(size: u64) -> String {
    const KB: u64 = 1024;
    // Tenths, in integers: a document's size is never near u64's limit.
    let tenths = |unit: u64| {
        let scaled = size.saturating_mul(10) / unit;
        format!("{}.{}", scaled / 10, scaled % 10)
    };
    if size < KB {
        format!("{size} B")
    } else if size < KB * KB {
        format!("{} KB", tenths(KB))
    } else {
        format!("{} MB", tenths(KB * KB))
    }
}

/// A web request's card: Allow, and Always allow when a rule was drafted.
fn web(request: WebKind, target: &str, host: &str, always: &str, sentence: &str) -> Card {
    let mut answers = vec![CardAnswer {
        done: format!("reading {host}"),
        asks: Asks::Confirm {
            question: question(sentence),
            yes: "Yes, allow it".to_owned(),
        },
        ..answer("allow", "Allow")
    }];
    if !always.is_empty() {
        answers.push(CardAnswer {
            done: format!("allowed: {always}"),
            asks: Asks::Always { question: format!( "Always allow {always}? This request goes now, and any later one it covers runs without asking." ), yes: "Yes, always".to_owned() },
            ..answer("always", "Always allow")
        });
    }
    let what = match request {
        WebKind::Search => "Search",
        WebKind::Fetch => "URL",
    };
    Card {
        answers,
        ..card(
            "Web request",
            sentence,
            vec![field(what, target), field("Goes to", host)],
            "allow",
        )
    }
}

/// An email's card: every field whole, the body verbatim, and Send asking
/// who it goes to.
fn email(
    account: &str,
    (to, cc): (&[String], &[String]),
    subject: &str,
    body: &str,
    files: usize,
    held: &HeldIn,
) -> Card {
    let everyone = to.iter().chain(cc).cloned().collect::<Vec<_>>().join(", ");
    let to = to.join(", ");
    let mut fields = vec![field("From", account), field("To", &to)];
    if !cc.is_empty() {
        fields.push(field("Cc", &cc.join(", ")));
    }
    fields.push(field("Subject", subject));
    let named: Vec<String> = match held {
        HeldIn::Read { values } => values.iter().fold(Vec::new(), |mut named, value| {
            let value = format!("your {}", value.named);
            if !named.contains(&value) {
                named.push(value);
            }
            named
        }),
        HeldIn::Unreadable { .. } => Vec::new(),
    };
    let mut carrying = named;
    match files {
        0 => {}
        1 => carrying.push("1 attachment".to_owned()),
        n => carrying.push(format!("{n} attachments")),
    }
    let carrying = match carrying.as_slice() {
        [] => String::new(),
        some => format!(" with {}", some.join(" and ")),
    };
    let sentence = match held {
        HeldIn::Read { values } if values.is_empty() => String::new(),
        HeldIn::Read { values } => format!(
            "Includes {}. Everyone on To and Cc receives it: {everyone}. The box writes the real \
             value into the message only when you tap Send.",
            values
                .iter()
                .map(|value| format!("your {}, {}", value.named, value.from))
                .collect::<Vec<_>>()
                .join("; and ")
        ),
        HeldIn::Unreadable { why } => format!("{why}. Send refuses it."),
    };
    Card {
        body: body.to_owned(),
        answers: vec![CardAnswer {
            done: format!("sent to {to}"),
            asks: Asks::Confirm {
                question: format!("Send to {to}{carrying}?"),
                yes: "Yes, send it".to_owned(),
            },
            ..answer("send", "Send")
        }],
        uncertain: "It may have been sent. Check Sent; it is never sent again.".to_owned(),
        sentence,
        ..card("Proposed email", "", fields, "send")
    }
}

/// A forward: an email's card, its original named above the note, and its question naming the
/// original and every held value the tap sends as the message holds it.
fn forward(
    email: Card,
    to: &str,
    original: &str,
    style: Style,
    held: &[String],
    files: &[CardFile],
) -> Card {
    let how = match style {
        Style::Inline => "below the note",
        Style::Attached => "attached as an .eml file",
    };
    let mut fields = email.fields;
    fields.push(field("Forwards", &format!("{original}, {how}")));
    let holding = if held.is_empty() {
        String::new()
    } else {
        fields.push(field("Held values in its text", &held.join(", ")));
        format!(" Its text includes your {}.", held.join(", "))
    };
    Card {
        title: "Proposed forward".to_owned(),
        files: files.to_vec(),
        answers: vec![CardAnswer {
            done: format!("forwarded to {to}"),
            asks: Asks::Confirm {
                question: format!("Forward {original} to {to}?{holding}"),
                yes: "Yes, forward it".to_owned(),
            },
            ..answer("send", "Forward")
        }],
        fields,
        ..email
    }
}

/// A card with the defaults every kind shares: no body, rows or problem, and the read-only line
/// in the main verb's words.
fn card(title: &str, sentence: &str, fields: Vec<CardField>, verb: &str) -> Card {
    Card {
        title: title.to_owned(),
        by: Asker::Secretary {},
        // An act's sentence is an imperative written to follow a name; on a card it stands alone.
        sentence: crate::secretary::noticed::said(sentence),
        fields,
        body: String::new(),
        rows: Vec::new(),
        files: Vec::new(),
        problem: String::new(),
        answers: Vec::new(),
        uncertain: "It may have gone through; it is never done again.".to_owned(),
        readonly: format!(
            "This device can read this but not {verb} it. Use one of your own devices."
        ),
        link: None,
    }
}

/// A secret: a labelled field the value goes in, and the template's words on where to find it;
/// its stored name is the box's alone and never drawn.
fn secret(label: &str, found: Option<&str>, link: Option<&str>) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: format!("stored {label}"),
            asks: Asks::Value {
                input: capitalised(label),
            },
            ..answer("store", "Store")
        }],
        link: link.map(|href| CardLink {
            label: "Where to get it".to_owned(),
            href: href.to_owned(),
        }),
        ..card(
            &format!("Store {label}"),
            found.unwrap_or_default(),
            Vec::new(),
            "store",
        )
    }
}

/// A secret the box asks for, as the wire carries it on a deployment whose `[coding]` declares
/// `routine` and whose model's credential is stored under `model`: in the owner's words, its
/// stored name left on the box.
#[must_use]
pub fn secret_act(name: &str, routine: Option<&RoutineId>, model: Option<&str>) -> ProposedAct {
    if model == Some(name) {
        return ProposedAct::Secret {
            label: "the model's sign-in".to_owned(),
            found: Some("Sign the model in from the Connect a model card.".to_owned()),
            link: None,
        };
    }
    let help = crate::settings::Help::of(name, routine);
    ProposedAct::Secret {
        label: crate::settings::Help::label(name),
        found: help.as_ref().map(|help| help.text.to_owned()),
        link: help.and_then(|help| help.link),
    }
}

/// `words` with its first letter in capitals, for a label that opens a line.
fn capitalised(words: &str) -> String {
    let mut chars = words.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Connect a model: the tap starts the deployment's sign-in.
fn sign_in(sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "sign-in started".to_owned(),
            asks: Asks::Confirm { question: "Start the sign-in? The box runs the model's sign-in and gives you its link to open." .to_owned(), yes: "Yes, start it".to_owned() },
            ..answer("connect", "Connect")
        }],
        ..card("Connect a model", sentence, Vec::new(), "connect")
    }
}

/// The secret store, locked: the tap asks it again once it is unlocked at the box.
fn unlock_store(sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "the store answers".to_owned(),
            asks: Asks::Now,
            ..answer("check", "Check again")
        }],
        ..card("Unlock the secret store", sentence, Vec::new(), "unlock")
    }
}

/// A changeset to a chain, row by row.
fn changeset(changes: &[crate::wire::ProposedObject], problem: &str) -> Card {
    let n = changes.len();
    Card {
        rows: changes.to_vec(),
        problem: problem.to_owned(),
        answers: vec![CardAnswer {
            done: "applied".to_owned(),
            asks: Asks::Confirm {
                question: if n == 1 {
                    "Apply this change?".to_owned()
                } else {
                    format!("Apply these {n} changes?")
                },
                yes: "Yes, confirm it".to_owned(),
            },
            ..answer("confirm", "Confirm")
        }],
        uncertain: "Some of it may already be applied. Check the todos, then dismiss it."
            .to_owned(),
        ..card("Proposed changes", "", Vec::new(), "confirm")
    }
}

/// The turns around a forgotten fact, offered for erasure.
fn erase(sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "erased".to_owned(),
            asks: Asks::Confirm {
                question: format!(
                    "{} {}",
                    question(sentence),
                    crate::secretary::erasure::Erasure::FOR_GOOD
                ),
                yes: "Yes, erase them".to_owned(),
            },
            ..answer("erase", "Erase")
        }],
        ..card(
            "Erase what was said around it",
            sentence,
            Vec::new(),
            "erase",
        )
    }
}

/// The code a sign-in waits for: its link, the box's own reading of the CLI, and a field.
fn sign_in_code(url: &str, sentence: &str) -> Card {
    Card {
        answers: vec![CardAnswer {
            done: "signed in".to_owned(),
            asks: Asks::Value {
                input: "Code from the sign-in page".to_owned(),
            },
            ..answer("finish", "Finish")
        }],
        link: Some(CardLink {
            label: "Sign in".to_owned(),
            href: url.to_owned(),
        }),
        ..card(
            "Connect a model",
            sentence,
            vec![field(
                "Then",
                "Sign in on the page the button opens, copy the code it shows, and paste it here.",
            )],
            "finish",
        )
    }
}

/// The answer `verb`, drawn as `label`, tapped at once; a caller says what it asks first.
fn answer(verb: &str, label: &str) -> CardAnswer {
    CardAnswer {
        verb: verb.to_owned(),
        label: label.to_owned(),
        done: String::new(),
        asks: Asks::Now,
    }
}

fn field(label: &str, value: &str) -> CardField {
    CardField {
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

/// A sentence ("open https://…") as a question ("Open https://…?").
fn question(sentence: &str) -> String {
    let mut chars = sentence.chars();
    chars.next().map_or_else(String::new, |first| {
        format!("{}{}?", first.to_uppercase(), chars.as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The confirm step's question an answer asks, or "" for one with none.
    fn asked(answer: &CardAnswer) -> &str {
        match &answer.asks {
            Asks::Confirm { question, .. }
            | Asks::Always { question, .. }
            | Asks::Endorse { question, .. } => question,
            Asks::Now | Asks::Value { .. } => "",
        }
    }

    /// Whether an answer's tap carries `always`.
    fn widens(answer: &CardAnswer) -> bool {
        matches!(answer.asks, Asks::Always { .. })
    }
    use crate::secretary::inbox_reading::Answer;
    use crate::secretary::responded::{Done, Responded, Tried};

    fn web(always: &str) -> ProposedAct {
        ProposedAct::Web {
            request: WebKind::Fetch,
            target: "https://www.gov.uk/tenancy-deposit-protection".to_owned(),
            host: "www.gov.uk".to_owned(),
            always: always.to_owned(),
            sentence: "open https://www.gov.uk/tenancy-deposit-protection".to_owned(),
        }
    }

    #[test]
    fn an_email_card_shows_every_field_whole_and_asks_who_it_goes_to() {
        let card = of(&ProposedAct::Email {
            account: "me@example.test".to_owned(),
            to: vec!["a@example.test".to_owned(), "b@example.test".to_owned()],
            cc: Vec::new(),
            subject: "The permit".to_owned(),
            body: "the whole body, never clamped".to_owned(),
            attachments: Vec::new(),
            held: HeldIn::Read { values: Vec::new() },
        });
        let labels: Vec<&str> = card.fields.iter().map(|f| f.label.as_str()).collect();
        assert_eq!(
            labels,
            ["From", "To", "Subject"],
            "no Cc when there is none"
        );
        assert_eq!(card.body, "the whole body, never clamped");
        assert_eq!(card.answers[0].verb, "send");
        assert_eq!(
            asked(&card.answers[0]),
            "Send to a@example.test, b@example.test?"
        );
    }

    /// A patch's card is the whole diff, and a public repository's says so in the question.
    #[test]
    fn a_patch_card_shows_the_diff_and_says_public_before_the_push() {
        let patch = |visibility| {
            of(&ProposedAct::Patch {
                repo: "bmabsout/resume_typst".to_owned(),
                visibility,
                base: "a".repeat(40),
                message: "Retitle the CV".to_owned(),
                files: vec!["cv.typ".to_owned()],
                patch: "--- a/cv.typ\n+++ b/cv.typ\n@@ -1 +1 @@\n-a\n+b\n".to_owned(),
                sentence: "open a pull request on bmabsout/resume_typst (1 file): Retitle the CV"
                    .to_owned(),
            })
        };
        let public = patch(Visibility::Public);
        assert!(public.body.contains("+b"));
        assert_eq!(public.answers[0].verb, "open");
        assert!(asked(&public.answers[0]).contains("anyone can read it"));
        assert!(!asked(&patch(Visibility::Private).answers[0]).contains("anyone"));
    }

    #[test]
    fn a_secret_asks_for_its_value_with_no_second_press() {
        let card = of(&secret_act("NTFY_TOPIC", None, None));
        let store = &card.answers[0];
        assert_eq!(
            (store.verb.as_str(), &store.asks),
            (
                "store",
                &Asks::Value {
                    input: "Your notifications topic".to_owned()
                }
            ),
            "a value is asked for with no confirm step"
        );
        assert_eq!(card.title, "Store your notifications topic");
        assert!(
            !format!("{card:?}").contains("NTFY_TOPIC"),
            "the stored name is nowhere on the card"
        );
        assert_eq!(
            card.readonly,
            "This device can read this but not store it. Use one of your own devices."
        );
        assert!(card.fields.is_empty(), "the help is the card's sentence");
        assert!(!card.sentence.is_empty(), "{card:?}");
        assert_eq!(
            card.link.map(|link| link.href).as_deref(),
            Some("https://docs.ntfy.sh/subscribe/phone/"),
            "the page that issues it is the card's link"
        );
    }

    /// The model's credential, whatever name the deployment stores it under, is worded as the
    /// model's sign-in.
    #[test]
    fn the_model_s_credential_is_worded_as_its_sign_in() {
        let card = of(&secret_act("ANY_NAME", None, Some("ANY_NAME")));
        assert_eq!(card.title, "Store the model's sign-in");
        assert!(!format!("{card:?}").contains("ANY_NAME"), "{card:?}");
    }

    #[test]
    fn a_document_request_names_the_file_its_folder_and_size_and_says_it_redacts() {
        let card = of(&ProposedAct::Drive {
            path: "/my-files/Docs/w2.pdf".to_owned(),
            name: "w2.pdf".to_owned(),
            folder: "/my-files/Docs".to_owned(),
            size: bytes(49_356),
            sentence: "read the text of w2.pdf in /my-files/Docs".to_owned(),
        });
        assert_eq!(card.title, "Document request");
        let fields: Vec<(&str, &str)> = card
            .fields
            .iter()
            .map(|f| (f.label.as_str(), f.value.as_str()))
            .collect();
        assert_eq!(
            fields,
            [
                ("File name", "w2.pdf"),
                ("Folder", "/my-files/Docs"),
                ("Size", "48.1 KB"),
            ]
        );
        assert_eq!(card.answers[0].verb, "read");
        assert_eq!(card.answers[0].label, "Allow once");
        assert!(asked(&card.answers[0]).contains("[redacted]"));
        let always = &card.answers[1];
        assert!(widens(always));
        assert_eq!(always.label, "Always allow this folder");
        assert_eq!(always.done, "allowed: /my-files/Docs");
        assert_eq!(bytes(812), "812 B");
        assert_eq!(bytes(3 * 1024 * 1024 + 200_000), "3.1 MB");
    }

    /// An email's files are all on its card, and its question counts them.
    #[test]
    fn an_email_card_lists_every_attachment_and_its_question_counts_them() {
        let notice = crate::wire::CardFile {
            name: "notice.pdf".to_owned(),
            media: "application/pdf".to_owned(),
            size: "1.2 MB".to_owned(),
            sha256: "a".repeat(64),
            from: crate::wire::FileFrom::Mail {
                message: "m1@example.test".to_owned(),
            },
        };
        let card = of(&ProposedAct::Email {
            account: "me@example.test".to_owned(),
            to: vec!["a@example.test".to_owned()],
            cc: Vec::new(),
            subject: "The notice".to_owned(),
            body: "attached".to_owned(),
            attachments: vec![notice.clone()],
            held: HeldIn::Read { values: Vec::new() },
        });
        assert_eq!(card.files, vec![notice]);
        assert_eq!(
            asked(&card.answers[0]),
            "Send to a@example.test with 1 attachment?"
        );
    }

    /// After a failure that sent nothing the card offers Try again, never "Always allow"; after
    /// one that may have gone, and once settled, it offers no yes at all.
    #[test]
    fn a_card_offers_try_again_only_after_nothing_went() {
        let email = of(&ProposedAct::Email {
            account: "me@example.test".to_owned(),
            to: vec!["a@example.test".to_owned()],
            cc: Vec::new(),
            subject: "The permit".to_owned(),
            body: "the body".to_owned(),
            attachments: Vec::new(),
            held: HeldIn::Read { values: Vec::new() },
        });
        let one = |response| {
            Reading::One(Answer {
                name: prodrome_core::event::Hash::new("a".repeat(64)).expect("hex"),
                at: prodrome_core::literal::Datetime::new(2026, 9, 28, 9, 0, 0, 0)
                    .expect("an instant"),
                response,
            })
        };
        let tried = |reason: &str| {
            one(Responded::Tried {
                attempt: 1,
                tried: Tried::NotSent {
                    reason: reason.to_owned(),
                },
            })
        };
        let again = at(email.clone(), &tried("auth: 535"));
        assert_eq!(again.answers.len(), 1);
        assert_eq!(again.answers[0].label, "Try again");
        assert_eq!(again.answers[0].verb, "send");
        assert_eq!(asked(&again.answers[0]), "Send to a@example.test?");
        assert_eq!(
            again.uncertain,
            "It may have been sent. Check Sent; it is never sent again."
        );
        assert_eq!(at(email.clone(), &Reading::Standing), email);
        let unreadable = Reading::Unreadable(crate::secretary::inbox::Unchecked);
        assert!(at(email.clone(), &unreadable).answers.is_empty());
        for settled in [
            Responded::Done(Done::MayHaveGone {
                reason: "the connection closed".to_owned(),
            }),
            Responded::Done(Done::Sent {
                recipients: Vec::new(),
                message_id: "<1@box>".to_owned(),
            }),
            Responded::Dismissed {
                reason: "wrong person".to_owned(),
            },
            Responded::Withdrawn(crate::secretary::responded::Withdrawal::Superseded),
        ] {
            assert!(
                at(email.clone(), &one(settled.clone())).answers.is_empty(),
                "{settled:?}"
            );
        }
        let ruled = at(
            of(&web("any page on gov.uk or its subdomains")),
            &tried("timed out"),
        );
        assert_eq!(ruled.answers.len(), 1);
        assert!(!widens(&ruled.answers[0]));
    }

    #[test]
    fn a_web_request_offers_always_allow_only_with_a_drafted_rule() {
        let plain = of(&web(""));
        assert_eq!(plain.answers.len(), 1);
        assert_eq!(
            asked(&plain.answers[0]),
            "Open https://www.gov.uk/tenancy-deposit-protection?"
        );
        assert_eq!(plain.fields[0].label, "URL");
        let ruled = of(&web("any page on gov.uk or its subdomains"));
        let always = &ruled.answers[1];
        assert!(widens(always));
        assert_eq!(always.verb, "always");
        assert_eq!(always.done, "allowed: any page on gov.uk or its subdomains");
    }
}
