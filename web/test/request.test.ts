import { test } from "node:test"
import assert from "node:assert/strict"

import http from "../src/wire/http"

const KEY = "1".repeat(64)

// The box names a request with `suzatary_signatures::signatures::request_name`, whose test pins the same name.
test("a request is named as the box names it", async () => {
  const at = "2026-10-06T09:00:00.000Z"
  const nonce = "0123456789abcdef0123456789abcdef"
  const named = (body: string, key = KEY, when = at, drawn = nonce) => http.requestName("POST", "/api/proposals/accept", when, drawn, body, key)
  assert.equal(await named(`{"objects":[]}`), "e6998a65333b75210595be2aed16bbcd715904c094b8a05199c1e515ad096a88")
  assert.notEqual(await named(`{"objects":[]}`, KEY, at, "f".repeat(32)), await named(`{"objects":[]}`), "two nonces ask twice")
  assert.notEqual(await named(`{"objects":["a"]}`), await named(`{"objects":[]}`))
  assert.notEqual(await named(`{"objects":[]}`, "2".repeat(64)), await named(`{"objects":[]}`), "two devices ask twice")
  assert.notEqual(await named(`{"objects":[]}`, KEY, "2026-10-06T09:00:01.000Z"), await named(`{"objects":[]}`), "two instants ask twice")
})
