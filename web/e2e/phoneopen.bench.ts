/**
 * WHAT A PHONE DOES ON OPENING ONE TODO, MEASURED: the list drawn and quiet at 412 px, a todo opened from it with a tap,
 * then three seconds for whatever the open starts. The bench says how many requests the page and its workers made for
 * the open, how many stream connections stand open before and after it, and how many page programs run, and how long
 * each answer of the talk took (`chat/talk.ts`'s `talk` measure). The first case opens the todo on a device that
 * holds the lines' skeleton and not their content (every content read held unanswered): the talk fetches nothing,
 * answers Quiet, and asks for no content. No gate runs it: `scripts/web-e2e.sh phoneopen` does, by name.
 */

import assert from "node:assert/strict"

import { BASE, openTodo, suite } from "./harness"
import cast from "../test/cast"

const app = await suite("phoneopen", [412, 900])
const { todos } = cast.world

const QUIET_MS = 3000
const streams = (open: readonly string[]): string[] => open.filter((r) => r.includes("events"))

type Talked = { readonly ms: number; readonly said: string }
const talked = `performance.getEntriesByName("talk", "measure").map((m) => ({ ms: Math.round(m.duration * 10) / 10, said: m.detail.said }))`

const opening = async (holdsContent: boolean): Promise<void> => {
  await app.page.attachWorkers()
  const journal = await app.page.journal()
  const release = holdsContent ? null : await app.page.holdRequests(["/api/stores/lines/content"])
  await app.page.navigate(`${BASE}/?holds=${holdsContent ? "all" : "skeleton"}#/`)
  await app.page.waitFor(`document.querySelectorAll(".tree .row").length > 0`, { timeout: 20000 })
  await new Promise((done) => setTimeout(done, QUIET_MS))
  const contentOf = (open: readonly string[]): string[] => open.filter((r) => r.includes("/lines/content"))
  const before = { content: contentOf(journal.inFlight()), streams: streams(journal.inFlight()), running: await app.page.evaluate<string>(`document.getElementById("view").dataset.running ?? "?"`) }
  const stop = await app.page.wire()
  await openTodo(app.page, todos.agents.id)
  await app.page.waitFor(`document.querySelector(".tile .todoview .todobody") !== null`, { timeout: 20000 })
  await new Promise((done) => setTimeout(done, QUIET_MS))
  const wired = stop()
  const after = { streams: streams(journal.inFlight()), running: await app.page.evaluate<string>(`document.getElementById("view").dataset.running ?? "?"`) }
  const by = (from: string): string[] => wired.filter((w) => w.from === from).map((w) => w.url.replace(/\?.*$/, ""))
  const talk = await app.page.evaluate<Talked[]>(talked)
  const content = contentOf(journal.inFlight())
  // Content reads the open began: none, whatever the device held.
  const asked = content.filter((r) => !before.content.includes(r))
  if (release !== null) await release()
  assert.ok(talk.length > 0, "the talk answered the open")
  assert.deepEqual(asked, [], "the open asked for no content")
  if (release !== null) assert.ok(talk.every((t) => t.said === "Quiet"), `the talk over a skeleton is Quiet: ${JSON.stringify(talk)}`)
  console.log(
    `phone open of one todo, ${holdsContent ? "the device holding the lines whole" : "the device holding the skeleton, not the content"}: ${JSON.stringify({
      talk,
      contentHeld: content.length,
      contentAsked: asked.length,
      requests: { page: by("page").length, worker: by("worker").length },
      urls: { page: by("page"), worker: by("worker") },
      streams: { before: before.streams.length, after: after.streams.length, open: after.streams },
      programs: { before: before.running, after: after.running },
    })}`,
  )
}

// The device holding nothing first, so the first case's holds only the skeleton.
app.scene("a phone opening one todo, the device holding the skeleton, not the content", () => opening(false))
app.scene("a phone opening one todo", () => opening(true))
