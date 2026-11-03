/**
 * Mail, a message and a thread, drawn by their Rust programs (`app/pages/src/{mail,message,thread}.rs`) in a real
 * browser against the real box's corpus of the seeded messages: a search is a navigation to its address, a row opens
 * its message, its body is text, and Back returns to the list.
 */

import assert from "node:assert/strict";

import { BASE, suite } from "./harness";
import cast from "../test/cast";

const { mail } = cast.world;
const [, ferret] = mail;

const app = await suite("mail");

const subjects = (): Promise<string[]> =>
  app.page.evaluate<string[]>(`[...document.querySelectorAll(".mailpage .maillist .mailsubject")].map((n) => n.textContent)`);

app.scene("Mail is a search away: the query goes to its own address, and the list holds what matches", async () => {
  await app.open(BASE);
  await app.page.evaluate(`location.hash = "#/mail"`);
  await app.page.waitFor(`document.querySelector(".mailpage .maillist") !== null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<string>(`document.querySelector(".mailpage h1.pagetitle").textContent`), "Mail");
  assert.equal(
    await app.page.evaluate<string | null>(`document.querySelector(".mailpage .chiprow .chipbtn.on")?.textContent ?? null`),
    "All",
    "every account is the chip that is on",
  );

  await app.page.type(".mailpage input.search", "ferret");
  await app.page.press("Enter");
  await app.page.waitFor(`location.hash === "#/mail?q=ferret"`);
  await app.page.waitFor(`document.querySelector(".mailpage h1.pagetitle")?.textContent === "Mail search"`, { timeout: 15000 });
  await app.page.waitFor(`document.querySelectorAll(".mailpage .maillist .mailrow").length === 1`, { timeout: 15000 });
  assert.deepEqual(await subjects(), [ferret.subject]);
  assert.equal(await app.page.evaluate<string>(`document.querySelector(".mailpage input.search").value`), "ferret", "the field holds the query");
  assert.match(await app.page.evaluate<string>(`document.querySelector(".mailpage .count").textContent`), /^1 message matching/);
});

app.scene("a row opens its message, whose body is text, and Back returns to the list", async () => {
  await app.open(BASE);
  await app.page.evaluate(`location.hash = "#/mail?q=ferret"`);
  await app.page.waitFor(`document.querySelectorAll(".mailpage .maillist .mailrow").length === 1`, { timeout: 15000 });
  await app.page.click(".mailpage .mailrow");
  await app.page.waitFor(`location.hash.startsWith("#/mail/")`);
  await app.page.waitFor(`document.querySelector(".mailmessage .mailbody") !== null`, { timeout: 15000 });
  assert.equal(await app.page.evaluate<string>(`document.querySelector(".mailmessage h1.pagetitle").textContent`), ferret.subject);
  assert.ok(
    (await app.page.evaluate<string>(`document.querySelector(".mailmessage .mailbody").textContent`)).includes(ferret.body),
    "the body is the message's text",
  );
  assert.equal(await app.page.evaluate<number>(`document.querySelectorAll(".mailmessage .mailbody *").length`), 0, "a body holds no element");
  const ask = await app.page.evaluate<string | null>(`document.querySelector(".mailmessage .asksecretary")?.getAttribute("href") ?? null`);
  assert.ok(ask?.startsWith("#/secretary?ref=mail%3A") === true, `Ask names the message by its hash: ${String(ask)}`);
  assert.equal(await app.page.evaluate<boolean>(`document.querySelector(".mailmessage details.why").open`), false, "the headers are folded");

  await app.page.click(".mailmessage button.back");
  await app.page.waitFor(`location.hash === "#/mail?q=ferret"`);
  await app.page.waitFor(`document.querySelectorAll(".mailpage .maillist .mailrow").length === 1`, { timeout: 15000 });
});
