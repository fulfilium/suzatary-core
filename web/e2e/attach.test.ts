/**
 * A proposed email carries the owner's own file, and the tap sends exactly its
 * bytes, as the tap's recorder (`--smtp-fixture`) writes them. A file changed
 * on disk after it was proposed makes the tap refuse with nothing recorded.
 */

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { PHONE, suite, WRITE_BASE, opened } from "./harness";
import cast from "../test/cast";

const { notice } = cast.world;

const app = await suite("attach");

const SENT = process.env["SUZATARY_E2E_SENT"] ?? "";
const MAIL = process.env["E2E_WRITE_MAIL"] ?? "";
const SHOTS = process.env["SUZATARY_SHOTS"] ?? join(tmpdir(), "suzatary-shots");
mkdirSync(SHOTS, { recursive: true });

const MESSAGE = join(MAIL, `${notice.mail.msgid}:2,S`);

const recorded = (subject: string): Record<string, unknown>[] =>
  readFileSync(SENT, "utf8")
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line) as Record<string, unknown>)
    .filter((sent) => sent["subject"] === subject);

/** The decoded bytes of the part named `name` in a MIME message, one character per byte. */
function part(mime: string, name: string): string {
  const boundary = /boundary="([^"]+)"/.exec(mime)?.[1];
  assert.ok(boundary !== undefined, "a multipart message");
  const named = mime.split(`--${boundary}`).find((p) => p.includes(`filename="${name}"`));
  assert.ok(named !== undefined, `a part named ${name}`);
  assert.match(named, /Content-Transfer-Encoding: base64/);
  assert.match(named, new RegExp(`Content-Type: ${notice.file.media.replace("/", "\\/")}`));
  const body = named.split(/\r?\n\r?\n/).slice(1).join("");
  return atob(body.replace(/\s+/g, ""));
}

async function converse(said: string): Promise<void> {
  await app.page.navigate(`${WRITE_BASE}/#/secretary/default`);
  await app.page.waitFor(`document.querySelector(".pagetitle") !== null`, { timeout: 15000 });
  const id = await app.page.evaluate<string>(
    opened("Untitled"),
  );
  await app.page.navigate(`${WRITE_BASE}/#/secretary/${id}`);
  await app.page.waitFor(`document.querySelector(".composebox") !== null`, { timeout: 8000 });
  await app.page.type(".composebox", said);
  await app.page.click(".composesend");
  await app.page.waitFor(`document.querySelector(".proposal .proposalfile") !== null`, { timeout: 20000 });
}

async function send(): Promise<string> {
  await app.page.click(".proposal .act-send");
  await app.page.waitFor(`document.querySelector(".proposalconfirm") !== null`, { timeout: 4000 });
  const asked = await app.page.evaluate<string>(`document.querySelector(".proposalconfirm").textContent`);
  await app.page.click(".proposal .act-send.confirming");
  return asked;
}

app.scene("the card lists the attachment, and the tap sends exactly its bytes", async () => {
  assert.notEqual(SENT, "", "the suite names a recorder file");
  await app.resize(PHONE);
  await converse("reply with the approval notice attached");

  // Every file is on the card before the tap, with a way to open where it comes from.
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposalfile .proposalfilename").textContent`),
    notice.file.name,
  );
  assert.equal(
    await app.page.evaluate<string>(`document.querySelector(".proposalfile .proposalfilemeta").textContent`),
    `${notice.file.media} · ${notice.file.content.length} B · sha256 ${createHash("sha256").update(notice.file.content, "utf8").digest("hex").slice(0, 12)}`,
  );
  assert.match(
    await app.page.evaluate<string>(`document.querySelector(".proposalfile .proposalfileopen").getAttribute("href")`),
    new RegExp(encodeURIComponent(notice.mail.msgid).replace(/\./g, "\\.")),
    "a mail attachment opens its message",
  );
  for (const theme of ["light", "dark"] as const) {
    await app.page.colorScheme(theme);
    await app.page.evaluate(`document.querySelector(".proposal").scrollIntoView({block: "start"})`);
    await app.page.screenshot(join(SHOTS, `attach-card-390-${theme}.png`));
  }
  await app.page.colorScheme(null);

  assert.match(await send(), /with 1 attachment\?$/, "the yes is to the message with its file");
  await app.page.waitFor(`document.querySelector(".proposal-sent") !== null`, { timeout: 15000 });

  const lines = recorded("The approval notice");
  assert.equal(lines.length, 1, "exactly one message would have gone");
  const [sent] = lines;
  assert.ok(sent !== undefined);
  const mime = String(sent["mime"]);
  assert.match(mime, /multipart\/mixed/);
  assert.match(mime, /Here is the approval notice you asked for\./, "the body comes first");
  assert.equal(part(mime, notice.file.name), notice.file.content, "the attachment is the source's bytes");
});

app.scene("a file changed after it was proposed is not sent, and neither is the message", async () => {
  assert.notEqual(MAIL, "", "the suite names the fixture's maildir");
  await app.resize(PHONE);
  await converse("draft the notice for the archive");
  const before = readFileSync(MESSAGE, "utf8");
  // The same message, its PDF swapped for other bytes after the card was drawn.
  const other = btoa(notice.file.content.replace("WAC0000000001", "WAC0000000002")).replace(/.{76}/g, "$&\r\n");
  const swapped = before.replace(
    /(filename="approval-notice\.pdf"\r?\nContent-Transfer-Encoding: base64\r?\n\r?\n)[A-Za-z0-9+/=\r\n]+?(\r?\n\r?\n--)/,
    `$1${other}$2`,
  );
  assert.notEqual(swapped, before, "the file on disk was changed");
  writeFileSync(MESSAGE, swapped);
  try {
    await send();
    await app.page.waitFor(`document.querySelector(".proposal-failed") !== null`, { timeout: 15000 });
    const outcome = await app.page.evaluate<string>(`document.querySelector(".proposal-failed .proposaloutcome").textContent`);
    assert.match(outcome, /approval-notice\.pdf changed since it was proposed/);
    assert.match(outcome, /nothing was sent/);
    assert.equal(recorded("The approval notice, for the archive").length, 0, "nothing was recorded");
    // Answered no, so the refused card leaves nothing standing for later scenes.
    await app.page.click(".proposal-failed .act-dismiss");
    await app.page.waitFor(`document.querySelector(".proposal-failed .dismissreason") !== null`, { timeout: 4000 });
    await app.page.type(".proposal-failed .dismissreason", "e2e cleanup");
    await app.page.click(".proposal-failed .act-dismiss.confirming");
    await app.page.waitFor(`document.querySelector(".proposal-dismissed") !== null`, { timeout: 15000 });
  } finally {
    writeFileSync(MESSAGE, before);
  }
});
