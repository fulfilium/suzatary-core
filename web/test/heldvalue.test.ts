// The placeholder grammar's browser half: the Rust's spellings (`suzatary_core::held_value`) read here, and the tokens print back to their input.

import { test } from "node:test"
import assert from "node:assert/strict"

import heldvalue from "../src/view/heldvalue"

// Character for character the spellings `held_value`'s own tests assert.
const REVEALABLE = "[SSN ending 3456 · tap to show · held:Zm9ybS0xQGV4YW1wbGUudGVzdA.0]"
const WITHHELD = "[ITIN ending 1234 · withheld]"
const CODE = "[verification code · tap to show · held:Zm9ybS0xQGV4YW1wbGUudGVzdA.1]"
const CARD = "[card ending 4242 · withheld]"
const FILE =
  "[file w2.pdf, 4.0 KB · tap to show · held:file:RnJvbU1haWwobWVzc2FnZT0nZm9ybS0xQGV4YW1wbGUudGVzdCcsIG5hbWU9J3cyLnBkZicp.file]"
const IN_FILE = "[SSN ending 6789 · tap to show · held:file:RnJvbU1haWwobWVzc2FnZT0nZm9ybS0xQGV4YW1wbGUudGVzdCcsIG5hbWU9J3cyLnBkZicp.0]"
const FILE_WITHHELD = "[file statement, May.pdf, under 1 KB · withheld]"
const DRIVE_FILE = "[file w2.pdf, 4.0 KB · tap to show · held:file:RnJvbURyaXZlKHBhdGg9Jy9teS1maWxlcy9Eb2NzL3cyLnBkZicp.file]"

test("the Rust's spellings read as held values, with and without a handle and a hint", () => {
  const ts = heldvalue.tokens(`mine is ${REVEALABLE}; also ${WITHHELD}, ${CODE} and ${CARD}.`)
  assert.deepEqual(
    ts.map((t) => heldvalue.fold(t, (x) => ["text", x], (kind, hint, handle) => [kind, hint, handle])),
    [
      ["text", "mine is "],
      ["SSN", "3456", "held:Zm9ybS0xQGV4YW1wbGUudGVzdA.0"],
      ["text", "; also "],
      ["ITIN", "1234", null],
      ["text", ", "],
      ["verification code", "", "held:Zm9ybS0xQGV4YW1wbGUudGVzdA.1"],
      ["text", " and "],
      ["card", "4242", null],
      ["text", "."],
    ],
  )
})

test("a file's placeholder reads as its name and size, and a value in its text as a number", () => {
  const ts = heldvalue.tokens(`see ${FILE}, ${IN_FILE} and ${FILE_WITHHELD}`)
  assert.deepEqual(
    heldvalue.tokens(DRIVE_FILE).map((t) => heldvalue.fold(t, (x) => [x], (kind, hint, handle) => [kind, hint, handle])),
    [["file", "w2.pdf, 4.0 KB", "held:file:RnJvbURyaXZlKHBhdGg9Jy9teS1maWxlcy9Eb2NzL3cyLnBkZicp.file"]],
  )
  assert.deepEqual(
    ts.map((t) => heldvalue.fold(t, (x) => ["text", x], (kind, hint, handle) => [kind, hint, handle])),
    [
      ["text", "see "],
      ["file", "w2.pdf, 4.0 KB", "held:file:RnJvbU1haWwobWVzc2FnZT0nZm9ybS0xQGV4YW1wbGUudGVzdCcsIG5hbWU9J3cyLnBkZicp.file"],
      ["text", ", "],
      ["SSN", "6789", "held:file:RnJvbU1haWwobWVzc2FnZT0nZm9ybS0xQGV4YW1wbGUudGVzdCcsIG5hbWU9J3cyLnBkZicp.0"],
      ["text", " and "],
      ["file", "statement, May.pdf, under 1 KB", null],
    ],
  )
})

test("the tokens print back to their input, whatever the text", () => {
  for (const s of [
    "",
    "no number here",
    REVEALABLE,
    `${REVEALABLE}${WITHHELD}`,
    `${FILE}${IN_FILE}${FILE_WITHHELD}`,
    "[file w2.pdf · withheld]",
    "[SSN ending 3456 · tap to show · held:bad base64!.0]",
    "[PIN ending 1234 · withheld]",
    "[SSN ending 3456 · tap to show ·",
  ]) {
    assert.equal(heldvalue.tokens(s).map(heldvalue.print).join(""), s)
  }
})

test("a spelling the box never writes stays text", () => {
  for (const s of [
    "[PIN ending 1234 · withheld]",
    "[SSN ending 3456 · tap to show · held:bad base64!.0]",
    "[SSN ending 12345 · withheld]",
    "[verification code ending · withheld]",
    "[file w2.pdf · withheld]",
    "[file w2.pdf, 12 KB · withheld]",
  ]) {
    assert.equal(heldvalue.holds(s), false, s)
  }
  assert.equal(heldvalue.holds(WITHHELD), true)
})
