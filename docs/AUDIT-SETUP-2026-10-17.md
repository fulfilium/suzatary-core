# First setup, as a new owner meets it — 2026-10-17

The owner asked, before the v2.0.0 tag: "Is the first setup process smooth?"
This file answers that question. It reports and fixes nothing: each finding
names a change, and the owner chooses which to take. It is the last lens of
the experience audit; the first is `docs/AUDIT-EXPERIENCE-2026-10-13.md`.

The pictures are named by file below. They are not in the repository; the
PR that adds this file says where they are.

## The answer, in four sentences

**No, not yet.** Connecting the model is smooth: four taps and one trip to
the provider's page, verified by a real call before it is kept. Everything
else a new owner connects — a calendar, a mail account's sending consent,
the Drive, a second model provider — still needs a hand edit of
`instance.toml` or a command at the box, and the app's own door for it,
"Ask to add one", opens a secretary that has no verb to add anything. The
first device cannot be trusted from the app at all, and until it is, every
card on the owner's own phone says "Use one of your own devices".

## How it was measured

- **Code.** origin/master at `3771081` (release 1.22.1). The fixture binary
  `suzatary-dev` (`scripts/app-build.sh --fixture`) and the production web
  bundle (`scripts/build-web.sh`). Setup stages 1 to 5 of
  `docs/SETUPAGENT-PLAN-2026-10-07.md` are in that tree; stage 6, the setup
  agent, is not. The first pictures (01 to 21) were taken with a bundle that
  also held #178's storesync change (a page left open across a later epoch
  waits for the store again); it does not touch any setup page, and the
  bundle was rebuilt on master for pictures 21 onward.
- **A fresh instance, as AGENT.md §4.4–4.6 makes one.** `suzatary-dev init`
  in an empty scratch directory, at a pseudo-terminal, answering its
  questions as Ada would; then `doctor`, `device init` and `verify`. Its
  secret store was a `dotenv` file of its own (`SECRETSPEC_PROVIDER`), and
  `HOME` and the XDG directories were scratch directories, so nothing touched
  this box's secretspec, units or data. The cast is the neutral one: Ada
  Lovelace, `ada@example.invalid`, `ada-box.tailnet.ts.net`.
- **The app over it.** `suzatary-dev serve` over that instance with the e2e
  harness's substitutions: `--whois-fixture ada@example.invalid`,
  `--model-fixture` (a script of my own: one line per ask below, each
  marked "(scripted)" where the words are mine and not a model's), and
  `--smtp-fixture`. Two edits a real box does not need: `web.app_url` and
  the port were pointed at loopback (`suzatary instance set`), because the
  harness serves plain http on loopback and the real unit serves the
  tailnet with TLS. Its `PATH` held the harness's fake `claude`, so the
  model's sign-in reached no real login.
- **Broken states.** Each connection was broken for real where the fixture
  allows it: the calendar's link pointed at a host that does not resolve and
  `plugin sync` run (the 15-minute timer's verb, with the built `ics.wasm`);
  the Drive CLI replaced by one that fails with "session expired"; the mail
  account synced with no sign-in (`suzatary sync`, which records
  `needs_login`). The revoked link (HTTP 401) and "no model answers" were
  read from the harness's own bootstrap fixture (`fixture-chain --settings
  --model unanswered --calendar revoked`), since a 401 needs a server the
  plugin may reach and the transport reaches only the public internet.
- **Driver.** Chromium over CDP through the harness's `web/e2e/cdp.ts`, in a
  throwaway script outside the repository; phone pictures at 390×844,
  `-full` pictures the whole scroll height, wide ones at 1440×900.
- **Times.** Wall clock on this container (4 cores, no CPU throttling, the
  box on loopback): from the tap to the first frame its answer is on screen.
  A person's own time (reading, typing, switching apps) is not in them; the
  words and taps are counted instead.

## The path, step by step

| # | Step | Where | Taps / typing | Machine time | Words to read | Leaves the app |
|---|---|---|---|---|---|---|
| 0 | Read AGENT.md | a terminal or a browser | — | — | 6,196 (about 25 minutes at 250 words a minute) | yes: the whole step |
| 1 | Prerequisites: NixOS, flakes, Tailscale, MagicDNS and HTTPS certificates, a coding agent | the system config, the Tailscale admin console | 4 config edits, 2 rebuilds | not measured | §1, 541 | yes |
| 2 | `init`, 7 questions | the box's terminal | 7 answers | 2.3 s | 21 lines | yes |
| 3 | `doctor`, `device init`, `verify` | the box's terminal | 3 commands | 0.1 s each | 60 lines of `doctor` | yes |
| 4 | First open of the app | the phone | 1 | Inbox 153 ms, Settings 137 ms | Todos 72; Inbox 111, then 113 more in its Setup group; Settings 345 over 3,413 px | — |
| 5 | Trust this phone | Settings → This device, then the terminal | 1 tap, then 64 hex characters typed at the box | card 1.4 s; trusted 0.45 s after the command | 42 on the card | yes: the terminal |
| 6 | Connect the model | Inbox or Settings | 4 taps (Connect, Yes, Sign in, Finish) and one code | link 0.76 s; done 1.1 s | 49 | yes: the provider's sign-in page, then back |
| 7 | Add a calendar | the terminal, then Settings, then the Inbox, then Settings | a hand edit of `instance.toml`, a server restart, 2 pastes of the link, 4 taps | each tap under 0.6 s; first check waits for the 15-minute timer | about 150 | yes: the terminal, and the calendar service's settings |
| 8 | Add a model provider (OpenRouter) | Secretary (a new topic), then Settings | 2 asks in 2 new topics, 4 taps to change, 1 paste and Store | 0.2–0.8 s each | about 120 | yes: the provider's key page |
| 9 | Allow a Drive folder | the terminal only | 2 hand edits, a restart | — | — | yes: the terminal, and the Drive CLI's own login |
| 10 | Mail's sign-in | Settings or Inbox | 2 taps (Run, Yes), then a device code typed elsewhere | not measured (scripted in the fixture) | 38 | yes: Microsoft's device-login page; sending's consent is the terminal only |

Counted across steps 2 to 10, a new owner leaves the app for the terminal
eight times (init, doctor and device init, the first device's key, the
calendar's table, the restart after it, the Drive's two keys, the restart
after them, the sending consent), and for a service's own page five times
(Claude's sign-in, the calendar's settings, OpenRouter's keys, Microsoft's
device login, the Drive CLI's login).

## Findings, ranked by impact

| Id | Impact | Step | Picture | What the person meets | Who objects, and why | Change |
|---|---|---|---|---|---|---|
| S1 | high | 7, 9 | `13-settings-offered.png`, `16-ask-calendar-answer.png` | Settings ends "Nothing connected for calendar, drive, notifications, coding sessions, other connections." with **Ask to add one**. It opens the Setup conversation with the secretary, whose verbs (`app/core/tests/secretary-surface.json`) hold no connect or templates verb; `propose_act` refuses `Connect`, `Disconnect` and `Grant` as "the box's own, or the setup agent's". So the only way to add a calendar, an account or a Drive folder is the hand edit AGENT.md §4.11 describes. The scripted answer in the picture says so; a real model following SECRETARY.md can say no more. | Norman: a control that cannot do what its label says is the widest gulf of execution there is. Krug: a dead end after the person did the obvious thing. Conversational designers (Erika Hall, Cathy Pearl): the agent is invited and cannot act, the one failure a conversation must never have. | Until stage 6 lands, draw an **Add** button per offered group that files that group's template `Connect` card itself, with no model, as the box already files fix cards; the card's parameters are the template's (a label for a calendar, an address for an account). Remove "Ask to add one" until the agent behind it can add. |
| S2 | high | 4, 5 | `02-inbox-first.png`, `05-trust-card.png`, `06-trust-card-details.png`, `04-settings-first-full.png` | On the owner's own new phone, every card ends "This device can read this but not run it. Use one of your own devices." This device is their own. Settings' last section says "ask, then tap the card in the Inbox on a trusted device", and no device is trusted yet. The card it files says the same to the same phone. The only way in is AGENT.md §4.10: run `./suzatary device trust <key>` at the box, and the 64-character key is behind the card's Details, to be copied from the phone to the box by hand. | Norman: the system's model ("an untrusted device") is not the person's ("my phone"), and the words follow the system's. Krug: a loop with no exit. Wroblewski: transcribing 64 hex characters from a phone is the worst input a form can ask for. | For an untrusted device, replace the line on every card with one sentence and one link: "This phone is not trusted yet. Trust it →", leading to This device. When the chain trusts no device, This device shows the first-device step itself: a short pairing code and the one command that takes it (`./suzatary device trust 4F7-K2Q`), the code resolving to the key on the box. Move This device to the top of Settings while it is untrusted. |
| S3 | high | 7, 8 | `15-setup-conversation-full.png`, `24-provider-refused-trace.png` | Settings' Ask opens the `setup` conversation. The box's own cards are filed into it as turns labelled **YOU DID** ("The box asks: …"), and each wakes the secretary, which answered twice in this run before the person said anything. Asking there to add OpenRouter was refused: "this round has read something that is not the owner's own words … a model setting is a vendor and a trust anchor the owner names himself". The same ask in a new topic worked at once. So the conversation Settings opens for changes is the one place a model setting cannot be changed. | Conversational designers: the system speaks in the person's voice ("YOU DID") and answers itself, so turn-taking is broken. Norman: an action refused for a reason the person caused by nothing they did. | Draw the box's turns as the box's ("The box asked"), never "YOU DID", and let them wake no model. Count the box's own cards as the owner's side for the Configure gate (they are the box's words, not mail or a page), or open Ask in a new conversation each time. |
| S4 | high | 8 | `26-provider-configure-card.png`, `28-provider-declared.png`, `29-provider-ordered.png` | "add OpenRouter" files a card that declares it. Seconds after the tap the box files a second card to remove it again ("no job orders the provider openrouter"), and asks for no key. The key card appears only after a second ask, in a second topic, ordered a job onto it and that card was tapped too. | Norman: the system undoes the act it just confirmed. Krug: three asks for one intention. Wroblewski: a multi-step form with no step indicator. | One Configure card for the intention: "Answer Chat with OpenRouter's <model>", declaring and ordering in one move, followed by the key card. Do not offer to remove an unused provider until it has been unused for a week. |
| S5 | high | 2 | (terminal) | `init` asks one question at a time and ends the whole run on the first answer it refuses. "London" for the zone ended it after two answers ("\"London\" names no time zone"), and "outlook" for the provider ended it after seven, with every answer typed lost. Nothing suggests `Europe/London` or `microsoft-personal`. | Wroblewski: inline validation and forgiving formats; never discard what was typed. Norman: an error should be a correction, not a restart. | Ask the same question again with the refusal above it. Suggest the closest valid answer ("London" → `Europe/London`; "outlook", "hotmail" → `microsoft-personal`). Offer the provider as a numbered choice. |
| S6 | high | 7, 9 | `17-calendar-as-other-connection.png` | AGENT.md §4.11's calendar is a hand edit of `instance.toml`. The running app did not see it until the server was restarted, and AGENT.md does not say to restart. The Drive is the same: `drive_cli` and `drive_allow` are hand edits, and the Drive line appears only once a folder is allowed. | Norman: no feedback that the edit took. Krug: "Is it on? Did it work?" | Until S1's Add exists, have the server read the instance file again when it changes, as it already does after a Configure tap (`State::reconnected`). Say in AGENT.md §4.11 what makes it take effect. |
| S7 | high | 7 | `22-calendar-unreachable.png`, `42-1440-settings.png` | A link to a host that does not resolve reads "Failing since 17:00 on 2026-10-17: lost (last tried 17:00 on 2026-10-17)." There is no card and no way to set a new link from the app: the class has no remedy, so the box files nothing. A mistyped link is a dead end until someone edits the secret at the box. | Norman: an error state with no way out. Krug: "lost" is a word of the code (`Answer::Lost`), not of the person. | Say what happened in the person's words ("The link's address could not be reached"). Whatever the class, a failing link always draws its Secret card, "Set the link again", since setting it again is always a valid move. |
| S8 | medium | 7 | `21-calendar-link-stored.png` | Storing a link answers "Done: stored your Work calendar's secret link", and the line then says "Nothing has checked it yet." The first check is the 15-minute timer. The plan's step 6 ("the box runs `Check::Synced` and appends … 14 events in the next 60 days") is not built. | Norman: feedback must come within the action's span. Conversational designers: confirm the outcome, not the input. | Run the template's check on the Store tap, and draw its result on the line ("Synced: 14 events in the next 60 days", or the failure and its card). |
| S9 | medium | 7 | `18a-calendar-secret-refused.png`, `18-calendar-grant-card.png`, `20-secret-card-after-grant.png` | A link from an origin the plugin may not reach is refused on the card ("Not sent, tried once: nothing was stored … a card asks to allow it", with **Try again**). The card that allows it is filed in the Inbox, not on the calendar's line in Settings where the person is. After Allow, the person goes back and pastes the link a second time. | Wroblewski: never make a person re-enter a value. Krug: the next step is on another page. Security: storing nothing until the origin is allowed is right, and it stays. | Draw the Grant card in place under the Secret card on the line. Keep the typed link in the tab's memory (never sent or stored) and resend it with the same tap when Allow succeeds. Word the refusal as a step, not a failure: "First allow cal.example.org", not "Not sent, tried once". |
| S10 | medium | 4 | `03-inbox-setup-open.png`, `04-settings-first-full.png` | Ada chose the subscription, and the box asks both to connect it and to store an Anthropic API key. Storage says "Kept on this box and pushed nowhere", and the box asks for a GitHub push token. Four of the first six cards ask for things this owner did not choose. | Krug: every unneeded card is a question the person must answer to know it is unneeded. Norman: the system's model (every declared name) is not the person's (what I chose). | File the API key's card only when an order names `anthropic/`, and the push token's only when the data repository has a remote on GitHub. |
| S11 | medium | 4 | `01-first-open.png`, `04-settings-first-full.png` | The first screen is an empty list and "4 wait on you" in small italic type; nothing says setup is unfinished or what to do next. Settings on a phone is 3,413 px and 345 words; the step that unblocks every tap (This device) is last, below the release plan (`suzatarytwo`, `suzatarytwoone`, `suzatarytwotwo`, "In progress. No items yet."). | Krug: the first page must make the next step obvious. Wroblewski: on a phone, what matters first goes first. | While anything is unconnected, the Todos page leads with one line: "Finish setting up · 3 of 6 done →", opening Settings. In Settings, order the groups by what blocks the others: This device (while untrusted), Models, Mail, Calendar, Drive, Storage. Move the release plan to About. |
| S12 | medium | 7 | `40-calendar-revoked-fixcard.png` | A calendar whose service answered 401 shows its fix card ("Store your Work calendar's secret link") and, directly under it, a card to remove the same calendar: "has synced … and holds nothing within 90 days of today". It holds nothing because it is failing. | Norman: two opposite actions offered for one state, each sensible alone. | Do not offer a calendar's removal while it is failing; count the 90 quiet days from the last successful sync. |
| S13 | medium | 9 | `31-drive-declared.png`, `33-drive-broken.png`, `34-drive-recovered.png` | A Drive CLI that said "session expired. Run 'proton-drive login' to sign in again." reads "Failing since 17:05: the Drive did not answer". No card, no command. After the CLI was repaired, the line still said "Failing" because the probe runs once an hour. | Norman: the system knows the remedy and withholds it. Krug: "did not answer" invites a reload, not a login. | Classify a CLI that asks for a login as `needs_login` and draw the terminal step (the Drive CLI's own sign-in command) behind "How to set this up at the box". Probe again when the line is opened, or offer "Check now". |
| S14 | medium | 2–3 | (terminal) | `doctor` right after `init` prints "door ada-box.tailnet.ts.net:443, but serve binds port 8787: it will not start". AGENT.md §4.5's check does not mention this line, and on a real box the unit binds differently, so it alarms for nothing. It also cites `docs/ARCHITECTURE.md §3` and a model-router plan (since deleted), and a missing model token shows only as "(none — claude-cli/claude-opus-5-5)". | Norman: a false alarm in the first check teaches the person to ignore checks. Krug: plan documents are not words for an owner. | When run outside the unit, say what the unit will bind, not what this shell would. Print "the model's token is not set: `./suzatary secret CLAUDE_CODE_OAUTH_TOKEN`, or connect it from the app" when it does not resolve. Drop the document references from `doctor`'s headings. |
| S15 | medium | 0 | (AGENT.md) | AGENT.md is 6,196 words, and the install starts in §4, after 2,794 words of needs, rules and choices; §4 itself is 2,237 words and 20 commands. It is written for a coding agent, which is right, but the person reads it too, since every step is said to them. | Krug: people scan; a guide is read in the order it is needed. | Lead with a one-screen outline: the six things the person will do themselves, each with how long it takes and where (terminal, Tailscale console, phone). Keep the rules in §2, and move "What leaves your box" to its own page linked from there. |
| S16 | medium | 5 | `08-inbox-trusted.png` | After the terminal trusted the key, the Inbox still holds the trust card, now with **Trust**, and the count rose from 4 to 5. | Norman: a stale request looks like a second request. | Withdraw a device's trust card when the chain trusts its key, as the box already withdraws a secret card nothing needs. |
| S17 | medium | 10 | `35-mail-needs-login.png`, `39-mail-after-grant.png` | The mail line says "Microsoft: reading not signed in, sending not signed in." Reading has a card. Sending's consent is a terminal step (`./suzatary auth <address> --scope smtp`) that the line neither offers nor names. | Norman: a visible state with no visible action. | Draw sending's consent as a terminal step under "How to set this up at the box", with its command, as Storage's line already does. |
| S18 | medium | 4–10 | `02-inbox-first.png`, `40-calendar-revoked-fixcard.png`, `26-provider-configure-card.png` | Words to decode: "ada@example.invalid is connected and holds no sign-in" (connected and not working); sentences in lower case ("no model has answered…", "authorise…"); "calendar/WORK has failed"; "the ics plugin"; "not declared → https://openrouter.ai/api/v1 with the OpenRouter key"; "Failing since … it answered 401 (…). It answered 401: …" (the cause twice); "Its key was refused: set a new one." above a card that signs in, not one that takes a key; "cannot work.." (two full stops). | Krug: self-evident words. Norman: the system's vocabulary leaks into the person's. | One pass over the setup words: sentence case; the label the person gave ("Work"), never `calendar/WORK`; "calendar" for the ics plugin; "add OpenRouter" for "declare"; each cause once; a remedy sentence that matches its card. |
| S19 | low | 8 | `24-provider-refused-trace.png` | The refusal shown to the person says "the owner names himself". SECRETARY.md speaks of `{{persona.name}}` as "he" throughout ("has no terminal on his phone"), so the secretary is told the owner is a man whoever the owner is. | Inclusive language; Krug (the person reads their own manual's assumptions about them). | Use they/them in SECRETARY.md and in refusal texts, or a `[persona]` pronoun the owner sets at `init`. |
| S20 | low | 2 | (terminal) | `init`'s "next, at a terminal:" stops at `doctor`. It does not mention that the model can be connected later from the app instead (AGENT.md §4.5 does), nor, for Gmail on a box with no browser, the SSH tunnel `gmi init` needs. | Norman: the next step should be in the place the person is looking. | Print both: "or skip this: the app will ask for it", and for Gmail "on a box with no browser, see AGENT.md §4.5 first". |
| S21 | low | 4 | `02-inbox-first.png` | The Inbox says "4 things waiting on you" and shows two, with "Setup · 2 things to set up" closed under them. Each card carries two tags ("App" and "Setup"), and the same cards appear again in Settings. | Krug; Nielsen's consistency heuristic. | One tag per card. Count what is shown ("2 waiting, and 2 setup steps"). |
| S22 | low | 10 | (from the code) | A mail account's device-code sign-in writes its URL and code as act turns in the Setup conversation (SECRETARY.md, "Proposing an act"). A person who tapped Run on the card in Settings has to go to Secretary to read them. The fixture's flow finishes at once, so this run did not show the wait. | Norman: the answer should appear where the question was asked. | Draw the URL and code on the card itself while the act waits, as the model's sign-in card already does (its **Sign in** button and code field). |

## Each lens

### Norman: gulfs, feedback, mapping

The model step is the one designed as Norman would: one card, its own
words, a button that opens exactly the page it needs, a field for the one
thing that comes back, and a check that proves it before anything is kept
(`12-settings-models-after.png`). Everywhere else the gulf of execution is
wide: the control that says "add" cannot add (S1), the card that says "use
your own device" is shown on it (S2), and a confirmed act is followed by an
offer to undo it (S4, S12). The gulf of evaluation is widest after a hand
edit: nothing says whether it took (S6), and a stored link is not checked
until the timer runs (S8). The best thing in the design is the fix card by
failure class (the 401 case in S12, `40-calendar-revoked-fixcard.png`): a failure
says its cause and offers its one remedy in place. It needs to cover every
class, and the unreachable link has no card.

### Krug: don't make me think

A new owner's first three pages ask them to decode the system: "This device
can read this but not run it", "is connected and holds no sign-in", "YOU
DID — The box asks", "the ics plugin", "not declared", "lost". Four of the
first six cards ask for things this owner did not choose (S10), so the
person must think about each to discover it does not apply. Settings is
scanned, not read, and at 3,413 px on a phone with the blocking step last
it cannot be scanned (S11). Krug's trunk test ("where am I, what can I do
here, what's next?") fails on the first screen: nothing says setup is not
finished.

### Wroblewski: forms and the phone

`init` is a form at a terminal, and it breaks his first rules: it throws
away every answer on one refusal and offers no correction (S5). In the app
the inputs are few and well made (one secure field per card, the model's
code field, a help link as a button), but a refused origin makes the person
paste the same link twice (S9), and the first device's trust asks them to
carry 64 characters from a phone to a terminal (S2). On a phone, Settings
puts the first thing to do last (S11). At 1440 px the overview is one
800 px column with nothing beside it (`42-1440-settings.png`), which the
first audit already raised (X4).

### Conversational setup (Erika Hall, Cathy Pearl)

A conversation for setup must know the state, never dead-end, take turns,
and confirm outcomes. Today's Setup conversation is a log, not a
conversation: the box's cards appear in the owner's voice and wake the
model, which answers to nobody (S3); asking it to add something reaches a
secretary with no way to do it (S1); and the one change it could make, a
model setting, is refused there and accepted in a fresh topic (S3). The
plan's stage 6 (a setup agent with `templates`, `propose_connect` and a
check verb) is what these designers would ask for, and it is not built. Its
design already answers most of S1, S3 and S8; the box's own cards (stages
1–5) answer the rest without a model, which is the better split: the model
is for what no template names.

## Where the lenses disagree

1. **Conversation against buttons.** The conversational designers want
   "Ask" to be the door; Krug and Norman want a visible **Add** per group,
   since a button says what it does and a text box does not. Both are
   possible because the cards are the same either way (the plan's §17
   answer 4). The recommendation is buttons now and Ask later (S1).
2. **Wroblewski against the security model on the refused link.** He would
   keep the link and store it once its origin is allowed; the design stores
   nothing from an origin the plugin may not reach, so a link cannot be kept
   on the box while its origin is unknown. Keeping it only in the tab's
   memory and resending it with the Allow tap satisfies both (S9).
3. **Norman against the trust model on the first device.** He would let the
   phone trust itself on first use; the design requires a trusted actor to
   vouch for every key, and for the first one that actor is the terminal. A
   short pairing code keeps the terminal as the authority and removes the
   transcription (S2).
4. **Krug against the evidence rule.** "Failing since 17:05 … (last tried
   17:05)" is the app showing its working, and Krug would drop the dates.
   Keep one ("since 17:05"), and drop the second when it is the same.

## Not covered

- **A fixture IMAP server.** None exists in this tree (`TcpListener`
  appears only in the plugin, coding and SMTP tests). Mail was broken by
  syncing an account with no grant, and its sign-in was the fixture's
  scripted device-code flow, which stores no grant, so the mail line still
  read "not signed in" after it (`39-mail-after-grant.png`); the run cannot
  say how the line reads once a real grant is stored.
- **The key test for a provider.** Under `--model-fixture` the scripted
  model also answers the test call a Secret card makes, so a fake
  OpenRouter key read "Answered" (`30-provider-key-tested.png`). The test's
  refusal path was not seen.
- **What a real model says in Setup.** The secretary's words in the
  pictures are the script's. The findings rest on its verbs and refusals,
  which are the box's.
- **The install itself.** NixOS, Tailscale's console, the NixOS module and
  its units, the first `nix run` of the app (which fetches or builds it),
  `deploy.sh` and the phone's trust over the tailnet were not run: this
  container has no systemd user manager or tailnet. Their words were read
  from AGENT.md and counted.
- **Dark mode and screen readers** were not checked in this lens.
