// One proposal, whole, and one implementation mounted in the conversation and the Inbox, so the
// confirmation never has two renderings. It knows no kinds: the box describes every card.
// A card is drawn closed: its head, why, what it would do and its answers. Every field opens under
// Details, whole and never clamped, and asking a yes opens it, so the confirmation is of exactly this text.
// The text is `textContent`: a model that read attacker-controlled mail wrote it, so never `rich.ts`.
// Confirming is a second press naming what would happen, not a `confirm()` the browser may truncate,
// and the tap carries an index and no body. Every answer is an intent: the card keeps no flag, and what
// it shows is the row, then the intents about it that the row does not yet hold.

import app from "../app/app";
import pairing from "../app/pairing";
import signer from "../prodrome/signer";
import { attachmentUrl, pairDevice, revealHeld } from "../wire/api";
import type { CardAnswer, CardFile, FileFrom, ProposalRow, ProposedObject, Revises } from "../wire/generated/wire";
import proposed from "./proposed";
import type { Child } from "../view/dom";
import { el } from "../view/dom";
import placeholder from "../view/placeholder";
import heldvalue from "../view/heldvalue";
import disclosure from "../view/disclosure";
import { noteField, noteOf } from "../view/noteline";
import coding from "./coding";
import cardui from "./cardui";
import storesync from "../prodrome/storesync";
import asks from "../prodrome/asks";
import intents from "../prodrome/intents";
import intentsync from "../prodrome/intentsync";
import vouch from "../prodrome/vouch";
import digest from "./digest";
import stage from "./stage";
import { anchor, parse, textWithReferences } from "../chat/refs";
import asker from "./asker";
import kept from "./kept";
import spoken from "./spoken";
import { hrefOf } from "../shell/route";
import { toast } from "../view/toast";

/** An answer with a confirm step: its question and its yes, and what its tap carries. */
type Confirmed = Extract<CardAnswer, { asks: "confirm" | "always" | "endorse" }>;
const confirmed = (a: CardAnswer): a is Confirmed => a.asks === "confirm" || a.asks === "always" || a.asks === "endorse";

function changedFields(object: ProposedObject): HTMLElement | null {
  const changes = proposed.changes(object);
  if (changes.length === 0) return null;
  return el(
    "dl",
    { class: "proposedchanges" },
    ...changes.flatMap((change) => [
      el("dt", null, change.field),
      el(
        "dd",
        null,
        el("span", { class: "was" }, change.before === "" ? "—" : change.before),
        el("span", { class: "arrow" }, "→"),
        el("span", { class: "now" }, change.after === "" ? "—" : change.after),
      ),
    ]),
  );
}

// Whether a row is one the card it replaces did not ask for.
export type Since = "First" | "Carried" | "New"
export const since = (revises: Revises | null, object: ProposedObject): Since =>
  revises === null ? "First" : object.position > revises.carried ? "New" : "Carried"

/** One event of a changeset, drawn as a row of its diff. */
export function objectRow(object: ProposedObject, from: Since = "First"): HTMLElement {
  const t = proposed.todo(object);
  const note = proposed.note(object);
  return el(
    "div",
    { class: `proposedobject act-${object.act}${from === "New" ? " proposednew" : ""}`, "data-object": String(object.position) },
    el(
      "p",
      { class: "proposedhead" },
      el("span", { class: "proposedact" }, digest.sentence(object)),
      from === "New" ? el("span", { class: "proposedfresh" }, "new") : null,
    ),
    t.body === "" ? null : el("p", { class: "proposedbody" }, t.body),
    note === null || note === "" ? null : el("p", { class: "proposednote" }, note),
    changedFields(object),
    t.pending_todo
      ? el(
          "p",
          { class: "proposedpending" },
          "This todo is not on the chain yet: another card adds it, and this one records an outcome for a todo with no content until that one is tapped.",
        )
      : null,
    el("a", { class: "proposedlink", href: hrefOf({ type: "todo", id: t.todo }) }, "open the todo"),
  );
}

/** A diff's overview: its one line, then whatever else the owner decides from, before any row of it. */
export const overview = (rows: readonly ProposedObject[], ...said: Child[]): HTMLElement => {
  const t = digest.of(rows);
  return el(
    "div",
    { class: "diffoverview" },
    el("p", { class: "diffline" }, digest.line(t)),
    said,
    [digest.urgentLine(t), digest.pendingLine(t)].filter((line) => line !== "").map((line) => el("p", { class: "diffflag" }, line)),
  );
};

/** Where a file opens: its message, the upload, its Drive path, its repository at a commit, or the copy allowed once. */
const opening = (from: FileFrom): HTMLElement =>
  from.kind === "repo"
    ? el("span", { class: "proposalfilepath" }, `${from.repo}: ${from.path} at ${from.at.slice(0, 12)}`)
    : from.kind === "mail"
    ? el("a", { class: "proposalfileopen", href: hrefOf({ type: "message", id: from.message }) }, "open the message")
    : from.kind === "upload"
      ? el("a", { class: "proposalfileopen", href: attachmentUrl(from.id), target: "_blank", rel: "noopener" }, "open the file")
      : from.kind === "kept"
        ? el("span", { class: "proposalfilepath" }, "the copy you allowed once")
        : el("span", { class: "proposalfilepath" }, from.path);

/** One attachment, whole: what leaves is what this row names. */
const fileRow = (file: CardFile): HTMLElement =>
  el(
    "li",
    { class: "proposalfile", "data-file": file.name },
    el("span", { class: "proposalfilename" }, file.name),
    el("span", { class: "proposalfilemeta" }, `${file.media} · ${file.size} · sha256 ${file.sha256.slice(0, 12)}`),
    opening(file.from),
  );

export interface ProposalFrame {
  /** A link back to the conversation; absent on the conversation page itself. */
  readonly context?: string;
  /** Ask about this one; absent where there is no compose box, so no Reply button lies. */
  readonly reply?: (proposal: ProposalRow) => void;
  /** What the tab has asked, the way the page read it; absent, the set now. */
  readonly intents?: intentsync.T;
  /** Each todo's title, so a source is named in words; one the page does not hold reads as "a todo". */
  readonly titles: spoken.Titles;
  /** `"row"` where the row the card stands in already says who asked (the Inbox's Setup row), so the card carries no tag of its own. */
  readonly askerSaidBy?: "row";
  /** What follows a performing answer the box took; absent, nothing. */
  readonly taken?: () => void;
}

function field(label: string, value: string): HTMLElement {
  return el(
    "div",
    { class: "proposalfield" },
    el("span", { class: "proposallabel" }, label),
    el("span", { class: "proposalvalue" }, value),
  );
}

const source = (text: string, titles: spoken.Titles): HTMLElement | string => {
  const r = parse(text);
  return r === null ? text : anchor(r, "reflink proposalsource", spoken.of(r, titles));
};

/** Answer, and say what came of it: the box's word for a taken answer, and the replica's for a lost one it now holds. */
const asking = (at: asks.Proposal, taken?: () => void) => (answer: asks.Answer, done: string, secret?: string): void =>
  void intentsync.ask({ type: "Answer", at, answer }, secret).then((o) => {
    if (o.type !== "Answered") return;
    if (o.answer.type === "Taken") {
      toast(done);
      if (answer.type === "Tap") taken?.();
    }
    else if (o.answer.type === "Lost" && o.held)
      void storesync
        .conversation(at.conversation, { limit: 1 }, null)
        .then((one) => (one.type === "Held" ? one.payload.proposals.find((p) => p.index === at.index) : undefined))
        .then((now) => (now === undefined ? undefined : toast(stage.said(now))));
  });

/** What an open intent draws in place of the answers, or the line above them when asking again is safe. */
const askedLine = (shown: intents.Shown): HTMLElement | null =>
  intents.fold<HTMLElement | null>(shown, {
    Absent: () => null,
    Pending: () => placeholder.inline(),
    Taken: () => placeholder.inline(),
    Failed: (f) =>
      el(
        "span",
        { class: "problem" },
        f.lost ? `No answer (${f.e}), and it was not done. Trying again never does it twice.` : f.e,
      ),
  });

const waits = (shown: intents.Shown): boolean => shown.type === "Pending" || shown.type === "Taken";

/**
 * The card, drawn from `proposal.card` and the intents about it alone. An answer with a `question` asks it before
 * performing; one with an `input` is a password field whose value travels only in the tap body.
 */
export function proposalCard(id: string, proposal: ProposalRow, frame: ProposalFrame): HTMLElement {
  const card = proposal.card;
  const at: asks.Proposal = { conversation: id, index: proposal.index, from: asks.signature(proposal) };
  const shown = intents.about(frame.intents ?? intentsync.held(), asks.about(id, proposal.index), asks.holdsRow(proposal));
  const ask = asking(at, frame.taken);
  const held = kept.key(id, proposal.index);
  // A card no longer answerable has nothing to send again.
  if (!stage.answerable(proposal.state)) kept.drop(held);
  const node = el("div", {
    class: `proposal proposal-${proposal.state} proposal-act-${proposal.act.kind}`,
    "data-proposal": String(proposal.index),
    "data-conversation": id,
    "data-key": `${id}/${proposal.index}`,
  });
  const tag = frame.askerSaidBy === "row" ? null : asker.tag(card.by);
  node.appendChild(
    el(
      "div",
      { class: "proposalhead" },
      el("span", { class: "proposalact" }, card.title),
      tag === null ? null : el("span", { class: "proposaltag", title: tag.tip }, tag.text),
      frame.context === undefined
        ? null
        : el(
            "a",
            { class: "proposalfrom", href: hrefOf({ type: "conversation", id, refs: [] }) },
            frame.context,
          ),
    ),
  );
  node.appendChild(el("p", { class: "proposalwhy" }, proposal.why));
  // A card that replaces another carries its rows first; the rows after them are what is new.
  if (proposal.revises !== null && proposal.revises.carried > 0 && proposal.revises.carried < card.rows.length)
    node.appendChild(el("p", { class: "proposalrevises" }, `Replaces an earlier card: the first ${proposal.revises.carried} of these were asked before, the rows marked new are not.`))
  // While its act runs, what the box waits for (a sign-in's link and code) is on the card where it was asked.
  if (proposal.announced !== null)
    node.appendChild(el("p", { class: "proposalannounced" }, textWithReferences(proposal.announced, "box")));
  if (card.sentence !== "") node.appendChild(el("p", { class: "proposalbody" }, card.sentence));
  // A changeset says what it changes in one line, and why it reached him, before its rows.
  if (card.rows.length > 0) node.appendChild(overview(card.rows));
  // The box's own table names this page, never a model's words.
  if (card.link !== null)
    node.appendChild(el("a", { class: "act act-help proposallink", href: card.link.href, target: "_blank", rel: "noopener noreferrer" }, card.link.label));
  // Why a changeset cannot apply is never folded away.
  if (card.problem !== "") node.appendChild(el("p", { class: "problem proposalproblem" }, `This changeset no longer applies: ${card.problem}`));
  // The step, the words and the details live in the card's register, so whatever redraws this card finds them where they were.
  const ui = cardui.of(cardui.key("proposal", `${id}/${proposal.index}`));
  const details = disclosure.details(
    cardui.details(ui),
    { class: "proposaldetails" },
    "Details",
    el(
      "div",
      { class: "proposalsources" },
      el("span", { class: "proposallabel" }, "Because of"),
      proposal.sources.map((text) => source(text, frame.titles)),
    ),
    card.fields.map((shown) => field(shown.label, shown.value)),
    card.body === "" ? null : heldvalue.within(el("pre", { class: "proposalbody" }, card.body), revealHeld),
    card.files.length === 0 ? null : el("ul", { class: "proposalfiles" }, card.files.map(fileRow)),
    // A changeset's rows, confirmed whole.
    card.rows.length === 0 ? null : el("div", { class: "proposedchangeset" }, ...card.rows.map(row => objectRow(row, since(proposal.revises, row)))),
  );
  node.appendChild(details);

  /** Reply attaches this proposal to the compose box, answered or standing. */
  const replyButton = (): HTMLElement | null => {
    const reply = frame.reply;
    if (reply === undefined) return null;
    return el("button", { type: "button", class: "act act-replyto", onclick: () => reply(proposal) }, "Reply");
  };

  if (proposal.state !== "standing") {
    node.appendChild(el("div", { class: "proposaloutcome" }, stage.said(proposal)));
  }
  const writes = app.writes(app.now());
  if (!stage.answerable(proposal.state)) {
    // The session a tapped coding proposal started: a field of the row, not a kind.
    if (proposal.coding !== null) {
      node.appendChild(
        coding.view(proposal.coding, shown, cardui.of(cardui.key("coding", `${id}/${proposal.index}`)), {
          answer: writes ? (a) => ask(a, "Dropped") : null,
        }),
      );
    }
    // A send that may have gone stays in front of him until he has checked Sent; its one answer sends nothing.
    const checking =
      proposal.state === "unknown" && writes
        ? waits(shown)
          ? askedLine(shown)
          : [
              askedLine(shown),
              el(
                "button",
                { type: "button", class: "act act-acknowledge", onclick: () => ask({ type: "Acknowledge" }, "Checked") },
                "I checked Sent",
              ),
            ]
        : null;
    const reply = replyButton();
    if (checking !== null || reply !== null) node.appendChild(el("div", { class: "proposalact-row" }, checking, reply));
    return node;
  }

  // A login the box names no actor for sees no button: the box would refuse the tap. A device whose key is not trusted
  // is sent to trust it, and on its own card is offered the box's code.
  if (!writes) {
    const now = app.now();
    const key = now.who.type === "Writes" ? now.who.key.type : null;
    const here = signer.signing();
    const settings = (words: string, link: string): HTMLElement =>
      el("div", { class: "proposaloutcome readonly" }, words, " ", el("a", { class: "act act-trustit", href: hrefOf({ type: "settings" }) }, link));
    node.appendChild(
      key === "Untrusted"
        ? settings("This device is not trusted yet.", "Trust it →")
        : key === "Revoked"
          ? settings("This device's key was revoked.", "Put a new key in its place →")
          : el("div", { class: "proposaloutcome readonly" }, card.readonly),
    );
    if (key === "Untrusted" && proposal.act.kind === "device" && here.type === "Held" && proposal.act.key === here.key)
      node.appendChild(pairing.view(cardui.of(pairing.KEY), pairing.acts(pairDevice, () => {})));
    return node;
  }

  const row = el("div", { class: "proposalact-row" });
  node.appendChild(row);
  if (waits(shown)) {
    row.append(...[askedLine(shown), replyButton()].filter((n) => n !== null));
    return node;
  }

  const go = (e: cardui.Event): void => {
    ui.go(e);
    details.toggleAttribute("open", ui.get().details === "Shown");
    drawRow();
  };
  // Asking closes the step: the intent is what shows from here on.
  const answering = (answer: asks.Answer, done: string, secret?: string): void => {
    ui.go({ type: "Closed" });
    ask(answer, done, secret);
  };

  const control = (answer: CardAnswer): HTMLElement => {
    if (answer.asks === "value") {
      const input = el("input", { type: "password", class: "secretvalue", autocomplete: "off", placeholder: "Paste it here" });
      const store = el("button", { type: "button", class: `act act-${answer.verb}`, disabled: true }, answer.label);
      // Only whether anything is typed is kept; the value is read once, at the tap, and never enters the ask or a register.
      // It is read again once the field is placed, since a card drawn again is handed what was typed without an event.
      const typed = (): void => void (store.disabled = input.value === "");
      input.addEventListener("input", typed);
      queueMicrotask(typed);
      store.addEventListener("click", () => {
        kept.keep(held, { value: input.value, done: answer.done });
        answering({ type: "Tap", note: "", always: false }, answer.done, input.value);
      });
      return el("div", { class: "proposalsecretfield" }, el("label", { class: "secretlabel" }, el("span", null, answer.input), input), store);
    }
    if (answer.asks === "now")
      return el("button", { type: "button", class: `act act-${answer.verb}`, onclick: () => answering({ type: "Tap", note: "", always: false }, answer.done) }, answer.label);
    return el("button", { type: "button", class: `act act-${answer.verb}`, onclick: () => go({ type: "Opened", step: { type: "Confirming", verb: answer.verb } }) }, answer.label);
  };

  // A yes only a trusted device can give: this device's key signs the registration the card asks for.
  const vouched = (answer: CardAnswer): void => {
    const act = proposal.act;
    if (act.kind !== "device") return;
    void vouch.endorsement(act.actor, act.key).then(
      (endorsement) => answering({ type: "Tap", note: noteOf(ui.get().text), always: false, endorsement }, answer.done),
      (e: unknown) => toast(e instanceof Error ? e.message : String(e)),
    );
  };

  // The confirm step names who it goes to, what it runs, what it applies.
  const confirming = (answer: Confirmed): Node[] => [
    el("span", { class: "proposalconfirm" }, answer.question),
    el(
      "button",
      {
        type: "button",
        class: `act act-${answer.verb} confirming`,
        onclick: () => (answer.asks === "endorse" ? vouched(answer) : answering({ type: "Tap", note: noteOf(ui.get().text), always: answer.asks === "always" }, answer.done)),
      },
      answer.yes,
    ),
    el("button", { type: "button", class: "act act-cancelsend", onclick: () => go({ type: "Closed" }) }, "No"),
    cardui.field(ui, noteField),
  ];

  // Saying no asks for the reason, which the secretary reads next round.
  const dismissing = (): Node[] => {
    const why = cardui.field(ui, () =>
      el("input", { type: "text", class: "dismissreason", placeholder: "Why not? (wrong person, already sent, not yet…)" }),
    );
    const confirm = (): void => {
      const reason = ui.get().text.trim();
      if (reason === "") why.focus();
      else answering({ type: "Dismiss", reason }, "dismissed");
    };
    return [
      why,
      el("button", { type: "button", class: "act act-dismiss confirming", onclick: confirm }, "Dismiss"),
      el("button", { type: "button", class: "act act-canceldismiss", onclick: () => go({ type: "Closed" }) }, "Keep"),
    ];
  };

  // A card with a problem offers no performing answer; the box would refuse the tap.
  const performing = card.problem === "" ? card.answers : [];
  function drawRow(): void {
    const t = ui.get();
    const asked = performing.filter(confirmed).find((a) => cardui.is(t, "Confirming", a.verb));
    row.replaceChildren(
      ...(asked !== undefined
        ? confirming(asked)
        : cardui.is(t, "Saying", "dismiss")
          ? dismissing()
          : [
              askedLine(shown),
              ...performing.map(control),
              el("button", { type: "button", class: "act act-dismiss", onclick: () => go({ type: "Opened", step: { type: "Saying", verb: "dismiss" } }) }, "Dismiss"),
              replyButton(),
            ]
      ).filter((n) => n !== null),
    );
  }
  drawRow();
  return node;
}
