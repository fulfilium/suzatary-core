// The core in its own thread: one module, the chains the tab holds read into it once per change, the lines pulled and
// read here, and every question about them answered here (`questions.serve`), so a page change never waits on the
// core's reading of a chain or of the lines.

import { loadCore } from "./core"
import questions from "./questions"
import signer from "./signer"
import http from "../wire/http"
import lo from "../algebra/lo"

// The DOM's types name `self` a window; a dedicated worker's scope answers on the same port it listens on.
const scope: questions.Port<questions.Asked, questions.Out> = self
const say = (said: questions.Said): void => scope.postMessage({ said })

// The worker's reads are signed with the key the page minted, read afresh and never minted here; what the box answers a
// revoked key or an outdated build is the page's to act on.
http.signWith(async () => {
  const key = await signer.existing()
  return key === null ? null : { key: key.public, sign: (name: string) => signer.sign(key, name) }
})
http.whenGone(() => say({ type: "Gone" }))
http.whenStale(() => say({ type: "Stale" }))

// A core that did not load fails every question, and is said once, so the page tells the box.
const ready = loadCore().then(core => questions.serve(core, say))
ready.catch((e: unknown) => say({ type: "Unloaded", why: lo.reason(e) }))
questions.listen(scope, ready)
