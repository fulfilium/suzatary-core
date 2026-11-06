// Every route the box answers, named once with its method and decoder.

import app from "../app/app";
import type { AboutPayload, AttachmentRow, Authoring, AuthoredPayload, BlockRender, BriefingPayload, BriefingsPayload, CalendarPayload, ConversationPayload, DevicePayload, DrawnPayload, Erasing, FactId, FoldPayload, MailSearchPayload, MePayload, MemoryPayload, NewTodo, ObjectsPayload, Overview, PostedObjects, Reasoning, ReceiptPayload, RecordPayload, Registering, ReleasesPayload, Revealing, SayPayload, Saying, Tapping, Titling, TodoPayload, UpdateTodo, Uploaded, Uploading, WaitingPayload } from "./generated/wire";
import { print } from "../chat/refs";
import type { Reference } from "../chat/refs";
import heads from "../prodrome/heads";
import http from "./http";
import storereads from "./storereads";
import type heldvalue from "../view/heldvalue";
import payload from "./generated/payload";

// Every ordinary page asks about the chain it is on; the Inbox asks every chain, so it names one.
const onChain = (route: string, chain?: string): string =>
  chain === undefined ? `${app.apiBase(app.now())}/${route}` : `/api/${encodeURIComponent(chain)}/${route}`;

export const fold = (chain?: string): Promise<FoldPayload> => http.get(onChain("fold", chain), payload.FoldPayload);

export const todo = (id: string): Promise<TodoPayload> =>
  http.get(onChain(`todo/${encodeURIComponent(id)}`), payload.TodoPayload);

/** Files the coding-session card; only a tap on that card starts a session. */
export const buildTodo = (id: string): Promise<ConversationPayload> =>
  http.act(onChain(`todo/${encodeURIComponent(id)}/build`), {}, payload.ConversationPayload);

/** `actor: null` is a viewer, who gets no act buttons. */
export const me = (): Promise<MePayload> => http.get(onChain("me"), payload.MePayload);

export const about = (): Promise<AboutPayload> => http.get("/api/about", payload.AboutPayload);
export const getReleases = (): Promise<ReleasesPayload> => http.get("/api/releases", payload.ReleasesPayload);

/** The objects the frontier `since` does not reach; none asks for all. */
export const objectsSince = (since: string, chain?: string): Promise<ObjectsPayload> => http.get(onChain(storereads.sinceIn("objects", since), chain), payload.ObjectsPayload);

// Our last sync's heads, so `/events` does not announce our write back; asking consumes them.
let ourHeads: heads.Heads | null = null;

export const wasOurWrite = (h: heads.Heads): boolean => {
  if (ourHeads === null || !heads.same(ourHeads, h)) return false;
  ourHeads = null;
  return true;
};

export const ours = (h: heads.Heads): void => {
  ourHeads = heads.of(h);
};

/** Literals only: the box rehashes each, since a name is what the bytes are, never what a client says. */
const posted = (path: string, objects: readonly string[], since: string): Promise<ReceiptPayload> =>
  http.act(path, { objects: [...objects], since } satisfies PostedObjects, payload.ReceiptPayload);

export const postObjects = (objects: readonly string[], since: string): Promise<ReceiptPayload> =>
  posted(onChain("objects"), objects, since).then(receipt => {
    if (receipt.heads.length > 0) ours(receipt.heads);
    return receipt;
  });

// Appends nothing: the replica seals the answer. No body names an actor; the box takes it from the gate's peer.
const author = (body: Authoring): Promise<AuthoredPayload> => http.act(onChain("author"), body, payload.AuthoredPayload);

export const authorTodo = (todo: NewTodo): Promise<AuthoredPayload> => author({ add: todo });
export const authorConfirm = (todo: string, note = ""): Promise<AuthoredPayload> => author({ confirm: { todo, note } });
/** Only the fields the caller sets travel; the rest stay as the box reads them. */
export const authorUpdate = (edit: UpdateTodo, note = ""): Promise<AuthoredPayload> => author({ update: { ...edit, note } });
// Undo asks the box even for a lifecycle event: undoing an edit needs the whole stream's folds, which the wasm lacks.
export const authorUndo = (todo: string): Promise<AuthoredPayload> => author({ undo: todo });
export const authorRedo = (todo: string): Promise<AuthoredPayload> => author({ redo: todo });

export const waitingCount = (): Promise<WaitingPayload> => http.get("/api/waiting", payload.WaitingPayload);

const paged = (account: string, before: string): string =>
  (account === "" ? "" : `&account=${encodeURIComponent(account)}`) +
  (before === "" ? "" : `&before=${encodeURIComponent(before)}`);

/** A notmuch query, passed through; `before` is the oldest row shown, "" for the newest. */
export const mailSearch = (q: string, account: string, limit: number, before = ""): Promise<MailSearchPayload> =>
  http.get(`/api/mail/search?q=${encodeURIComponent(q)}&limit=${limit}${paged(account, before)}`, payload.MailSearchPayload);

export const briefings = (): Promise<BriefingsPayload> => http.get("/api/briefings", payload.BriefingsPayload);

export const briefing = (date: string): Promise<BriefingPayload> =>
  http.get(`/api/briefings/${encodeURIComponent(date)}`, payload.BriefingPayload);
export const calendar = (date: string): Promise<CalendarPayload> =>
  http.get(`/api/calendar/${encodeURIComponent(date)}`, payload.CalendarPayload);


export const getOverview = (): Promise<Overview> => http.get("/api/overview", payload.Overview);

// Not under `app.apiBase`: a conversation reads every chain and the mail, so it is no chain's.

const conversationPath = (id: string, rest = ""): string =>
  `/api/secretary/conversations/${encodeURIComponent(id)}${rest}`;
const proposalPath = (id: string, index: number, verb: string): string =>
  conversationPath(id, `/proposals/${index}/${verb}`);

/** A blank title is left for the box to name after the first answer. */
export const openConversation = (title: string): Promise<ConversationPayload> =>
  http.act("/api/secretary/conversations", { title } satisfies Titling, payload.ConversationPayload);

export const renameConversation = (id: string, title: string): Promise<ConversationPayload> =>
  http.act(conversationPath(id, "/title"), { title } satisfies Titling, payload.ConversationPayload);

/** The owner's tap: the words are destroyed on the box and, at their next pull, on every device. */
export const eraseTurns = (id: string, turns: readonly number[]): Promise<ConversationPayload> =>
  http.act(conversationPath(id, "/erase"), { turns: [...turns] } satisfies Erasing, payload.ConversationPayload);

// The chips travel by value, not inside the sentence: one the box cannot resolve is a 400 rather than a dead link.
/** 202 with the person's turn; the model runs behind it. `after` is how many turns the asker saw, so the same question
 * asked twice gets the first one's line. */
export const say = (
  id: string,
  text: string,
  references: readonly Reference[] = [],
  attachments: readonly AttachmentRow[] = [],
  after?: number,
): Promise<SayPayload> =>
  http.act(conversationPath(id, "/say"), { ...asked(text, references, attachments), ...(after === undefined ? {} : { after }) } satisfies Saying, payload.SayPayload);

/** `say`'s body; the old question and all after it are set aside first. */
export const editQuestion = (
  id: string,
  text: string,
  references: readonly Reference[] = [],
  attachments: readonly AttachmentRow[] = [],
): Promise<ConversationPayload> =>
  http.act(conversationPath(id, "/edit"), asked(text, references, attachments), payload.ConversationPayload);

const asked = (text: string, references: readonly Reference[], attachments: readonly AttachmentRow[]): Saying => ({
  say: text,
  references: references.map(print),
  attachments: attachments.map(({ id, name }) => ({ id, name })),
});

/** A 409 in the box's words when nothing runs or waits. */
export const stopAnswer = (id: string): Promise<ConversationPayload> =>
  http.act(conversationPath(id, "/stop"), {}, payload.ConversationPayload);

export const regenerate = (id: string): Promise<ConversationPayload> =>
  http.act(conversationPath(id, "/regenerate"), {}, payload.ConversationPayload);

/** Drawn by the box, since no replica in the tab holds the todo chains whole. */
export const drawnProposals = (id: string, indices: readonly number[]): Promise<DrawnPayload> =>
  http.get(conversationPath(id, `/drawn?indices=${indices.join(",")}`), payload.DrawnPayload);

/** Named by its sha, never by its text, so the box renders only what the model wrote. */
export const block = (id: string, sha: string): Promise<BlockRender> =>
  http.get(conversationPath(id, `/blocks/${encodeURIComponent(sha)}`), payload.BlockRender);

export const blockBySource = async (id: string, source: string): Promise<BlockRender> =>
  block(id, [...new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(source)))].map((b) => b.toString(16).padStart(2, "0")).join(""));

/** Base64, since the write gate takes JSON only; a `say` then names the id. `private` sends a file the box held back
 * to the owner's Drive, not the box. */
export const upload = (base64: string, name: string, keepPrivate = false): Promise<Uploaded> =>
  http.act("/api/secretary/attachments", { file: base64, name, private: keepPrivate } satisfies Uploading, payload.Uploaded);

/** A send answers in seconds; past this the card asks the box what happened. */
export const TAP_BOUND = 30_000;

/** The irreversible act. It carries an index, never a body that could change what is performed; `value` is a secret
 * act's alone. */
export const tapProposal = (
  id: string,
  index: number,
  note = "",
  value?: string,
  always = false,
  endorsement?: { readonly key: string; readonly signature: string },
): Promise<ConversationPayload> =>
  http.act(
    proposalPath(id, index, "tap"),
    // `always` and `endorsement` only when set, so every other tap's body is unchanged.
    {
      note,
      ...(value === undefined ? {} : { value }),
      ...(always ? { always } : {}),
      ...(endorsement === undefined ? {} : { endorsement }),
    } satisfies Tapping,
    payload.ConversationPayload,
    TAP_BOUND,
  );

/** Behind the tap's own gate. */
export const dismissProposal = (id: string, index: number, reason: string): Promise<ConversationPayload> =>
  http.act(proposalPath(id, index, "dismiss"), { reason } satisfies Reasoning, payload.ConversationPayload);

/** The owner has checked a tap that may have gone through; it performs nothing. */
export const acknowledgeProposal = (id: string, index: number): Promise<ConversationPayload> =>
  http.act(proposalPath(id, index, "acknowledge"), {}, payload.ConversationPayload);

export const dropSession = (id: string, index: number, reason: string): Promise<ConversationPayload> =>
  http.act(proposalPath(id, index, "drop"), { reason } satisfies Reasoning, payload.ConversationPayload);

export const getMemory = (): Promise<MemoryPayload> => http.get("/api/secretary/memory", payload.MemoryPayload);

/** `conversation` is `null` on a mail page. Answered once and kept nowhere; `Gone` when the box holds it no more or
 * answered a replay. */
export const revealHeld = (conversation: string | null): heldvalue.Reveal => (handle) =>
  http.act("/api/held/reveal", { handle, conversation } satisfies Revealing, payload.RevealPayload).then((p): heldvalue.Opened => {
    switch (p.type) {
      case "Shown":
      case "File":
        return p;
      case "Gone":
      case "NotKept":
        return { type: "Gone" };
    }
  });

/** Signed here for the native code, which holds no key. */
export const reportPushToken = (token: string): Promise<DevicePayload> =>
  http.act("/api/act/device", { platform: "android", token } satisfies Registering, payload.DevicePayload);

export const forgetFact = (id: FactId): Promise<MemoryPayload> =>
  http.act(`/api/secretary/memory/${id}/forget`, {}, payload.MemoryPayload);

export const getRecord = (): Promise<RecordPayload> => http.get("/api/secretary/record", payload.RecordPayload);

export const retractSummary = (node: string): Promise<RecordPayload> =>
  http.act(`/api/secretary/record/${node}/retract`, {}, payload.RecordPayload);
