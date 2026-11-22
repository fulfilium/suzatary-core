//! The made-up world every fixture and test draws from, so the code tree names no real person;
//! every address is under a reserved domain.
//! Prose only: each chain builder prices, because a price is relative to the builder's clock.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Person {
    pub name: &'static str,
    pub email: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Owner {
    /// Trusted, in every roster.
    pub actor: &'static str,
    pub name: &'static str,
    /// The one mail account a fixture deployment has.
    pub email: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Sub {
    /// Typst markup.
    pub text: &'static str,
    pub done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Todo {
    pub id: &'static str,
    pub created: &'static str,
    /// Typst markup.
    pub body: &'static str,
    /// The markup's one `*strong*` span, as a page shows it, or "".
    pub strong: &'static str,
    /// Typst markup, or "".
    pub detail: &'static str,
    /// From the vocabulary the app orders (`CATEGORY_ORDER`).
    pub category: &'static str,
    pub rationale: &'static [&'static str],
    pub subtodos: &'static [Sub],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Mail {
    pub from: Person,
    pub subject: &'static str,
    /// The `Message-Id`, without angle brackets.
    pub msgid: &'static str,
    /// One line.
    pub body: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CastFile {
    pub name: &'static str,
    /// `type/subtype`.
    pub media: &'static str,
    /// The bytes, which for the cast's one PDF are ASCII.
    pub content: &'static str,
}

/// A message with one attachment, which a proposed reply attaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub mail: Mail,
    pub file: CastFile,
}

/// The six todos of the fixture chain, each carrying one thing a page has to get right.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Todos {
    /// A Decay two days from its deadline, which sorts first; its body holds markup.
    pub urgent: Todo,
    /// A Flat priced in the middle.
    pub flat: Todo,
    /// A checklist whose detail links to `flat`.
    pub checklist: Todo,
    /// Resolved the day after it was authored.
    pub resolved: Todo,
    /// Priced high, so "Done" removes a card from the middle of the list.
    pub high: Todo,
    /// The triage agent's, untrusted and unconfirmed, from [`Cast::mail`]'s first message; its
    /// markup shows as source.
    pub agents: Todo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Cast {
    pub owner: Owner,
    /// A law firm, the sender of the agent's todo.
    pub counsel: Person,
    /// Someone the secretary is asked to remember.
    pub accountant: Person,
    /// A friend whose mail matches no todo.
    pub friend: Person,
    pub city: &'static str,
    pub todos: Todos,
    /// The first is about [`Todos::agents`]; the second matches nothing local.
    pub mail: [Mail; 2],
    /// The todo the fixture's triage run proposes.
    pub proposed: Todo,
    /// The e2e fixture's one unpriced open todo: it reads `∅` and lists after every priced one.
    pub unpriced: Todo,
    /// A scanned approval notice the fixture's mail index also holds.
    pub notice: Notice,
}

const COUNSEL: Person = Person {
    name: "Harrow",
    email: "counsel@example.test",
};

const REGISTRY: Person = Person {
    name: "Nordvik Registry Office",
    email: "notices@registry.example.test",
};

/// A one-page PDF saying what the notice says, small enough to read in a diff.
const NOTICE_PDF: &str = "%PDF-1.4
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 144] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length 60 >> stream
BT /F1 12 Tf 24 100 Td (Approval notice WAC0000000001) Tj ET
endstream endobj
5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj
trailer << /Size 6 /Root 1 0 R >>
%%EOF
";

const FRIEND: Person = Person {
    name: "Diego",
    email: "diego@example.test",
};

pub const CAST: Cast = Cast {
    owner: Owner {
        actor: "ada",
        name: "Ada",
        email: "her@example.invalid",
    },
    counsel: COUNSEL,
    accountant: Person {
        name: "Mira Solberg",
        email: "mira@example.test",
    },
    friend: FRIEND,
    city: "Oslo",
    todos: Todos {
        urgent: Todo {
            id: "file-permit",
            created: "permit renewal",
            body: "File the *permit* renewal",
            strong: "permit",
            detail: "",
            category: "work",
            rationale: &["Missing it is not recoverable."],
            subtodos: &[],
        },
        flat: Todo {
            id: "book-flight",
            created: "",
            body: "Book the flight to Oslo",
            strong: "",
            detail: "",
            category: "home",
            rationale: &[],
            subtodos: &[],
        },
        checklist: Todo {
            id: "pack-bags",
            created: "",
            body: "Pack for the trip",
            strong: "",
            detail: "After #link(\"todo:book-flight\")[the flight] is booked.",
            category: "home",
            rationale: &[],
            subtodos: &[
                Sub {
                    text: "[tickets]",
                    done: true,
                },
                Sub {
                    text: "[chargers]",
                    done: false,
                },
            ],
        },
        resolved: Todo {
            id: "renew-card",
            created: "",
            body: "Renew the library card",
            strong: "",
            detail: "",
            category: "home",
            rationale: &[],
            subtodos: &[],
        },
        high: Todo {
            id: "water-plants",
            created: "",
            body: "Water the plants",
            strong: "",
            detail: "",
            category: "home",
            rationale: &[],
            subtodos: &[],
        },
        agents: Todo {
            id: "email-harrow",
            created: "",
            body: "Email *Harrow* about the deed",
            strong: "Harrow",
            detail: "",
            category: "other",
            rationale: &[],
            subtodos: &[],
        },
    },
    mail: [
        Mail {
            from: COUNSEL,
            subject: "Harrow asked for one more signature",
            msgid: "fixture-harrow-deed@example.test",
            body: "They are asking for one more signature before the deed is recorded.",
        },
        Mail {
            from: FRIEND,
            subject: "Quarterly ferret census results",
            msgid: "fixture-ferret-census@example.test",
            body: "Every burrow got counted twice this quarter, on purpose.",
        },
    ],
    proposed: Todo {
        id: "renewlease",
        created: "",
        body: "Renew the lease before the window closes",
        strong: "",
        detail: "",
        category: "home",
        rationale: &["an errand with a recoverable cost"],
        subtodos: &[],
    },
    unpriced: Todo {
        id: "tenancy-rules",
        created: "",
        body: "Read the tenancy rules",
        strong: "",
        detail: "",
        category: "study",
        rationale: &[],
        subtodos: &[],
    },
    notice: Notice {
        mail: Mail {
            from: REGISTRY,
            subject: "Your approval notice",
            msgid: "fixture-approval-notice@example.test",
            body: "The scanned approval notice is attached.",
        },
        file: CastFile {
            name: "approval-notice.pdf",
            media: "application/pdf",
            content: NOTICE_PDF,
        },
    },
};

impl Todos {
    /// In the order the chain appends them.
    #[must_use]
    pub fn all(&self) -> [&Todo; 6] {
        [
            &self.urgent,
            &self.flat,
            &self.checklist,
            &self.resolved,
            &self.high,
            &self.agents,
        ]
    }
}

/// The keys a scene's deployment signs with, as hex seeds; never a real deployment's, because
/// this file is public.
pub mod keys {
    use prodrome_core::sign::{PublicKey, Secret};

    /// The box's own key, whose public half a scene's `[signatures] root` names.
    pub const ROOT: &str = "ca57000000000000000000000000000000000000000000000000000000000b0c";
    /// The agent's key, registered for the agent's actor.
    pub const AGENT: &str = "ca57000000000000000000000000000000000000000000000000000000000a9e";
    /// The owner's phone, the device a scene's requests are signed by.
    pub const PHONE: &str = "ca57000000000000000000000000000000000000000000000000000000000f0e";
    /// The key the mail's dedup names are made with.
    pub const DEDUP: &str = "ca5700000000000000000000000000000000000000000000000000000000d0d0";

    /// # Panics
    /// Never for the seeds above, which are seeds.
    #[must_use]
    pub fn secret(seed: &str) -> Secret {
        Secret::new(seed).expect("a cast seed is a seed")
    }

    #[must_use]
    pub fn root() -> PublicKey {
        secret(ROOT).public()
    }
}
