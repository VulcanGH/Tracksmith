import assert from "node:assert/strict";
import { test } from "node:test";
import { draftReducer, emptyDraft, parseTsv, serializeTsv } from "./draft.ts";

test("coalesced typing, atomic bulk edits, undo/redo/revert and clipboard round-trip", () => {
  const patch = (uid: string, value: string) => ({
    edit: { track_uid: uid, property: "title" as const, value },
    original: null,
  });
  let state = draftReducer(emptyDraft(), {
    type: "apply",
    patches: [patch("1", "A")],
    group: "1:title",
  });
  state = draftReducer(state, {
    type: "apply",
    patches: [patch("1", "Atmos")],
    group: "1:title",
  });
  assert.equal(state.past.length, 1);
  state = draftReducer(state, { type: "commit" });
  state = draftReducer(state, {
    type: "apply",
    patches: [patch("1", "Stereo"), patch("2", "Stereo")],
  });
  state = draftReducer(state, { type: "undo" });
  assert.equal(state.present["1:title"].value, "Atmos");
  assert.equal(state.present["2:title"], undefined);
  state = draftReducer(state, { type: "redo" });
  assert.equal(state.present["2:title"].value, "Stereo");
  state = draftReducer(state, { type: "revert" });
  assert.deepEqual(state.present, {});
  state = draftReducer(state, { type: "undo" });
  assert.equal(state.present["2:title"].value, "Stereo");
  state = draftReducer(state, { type: "apply", patches: [patch("1", "New")] });
  assert.equal(state.future.length, 0);
  const rows = [
    ["A\tB", "en-GB"],
    ['Audio "title"\nsecond line', "fr"],
  ];
  assert.deepEqual(parseTsv(serializeTsv(rows)), rows);
  assert.deepEqual(parseTsv("A\ten\r\nB\tfr\r\n"), [
    ["A", "en"],
    ["B", "fr"],
  ]);
  assert.throws(() => parseTsv("a\tb\tc"));
  assert.throws(() => parseTsv('"unclosed'));
});

test("rules are replaceable proposals and manual original-value overrides survive reapplication and undo", () => {
  const original = null;
  const patch = (value: string | null) => ({
    edit: { track_uid: "1", property: "title" as const, value },
    original,
  });
  let state = draftReducer(emptyDraft(), {
    type: "rules",
    patches: [patch("Generated")],
  });
  assert.equal(state.present["1:title"].value, "Generated");
  state = draftReducer(state, { type: "apply", patches: [patch(null)] });
  assert.equal(state.present["1:title"], undefined);
  assert.equal(state.manual["1:title"].value, null);
  state = draftReducer(state, {
    type: "rules",
    patches: [patch("Regenerated")],
  });
  assert.equal(state.present["1:title"], undefined);
  state = draftReducer(state, { type: "release", keys: ["1:title"] });
  assert.equal(state.present["1:title"].value, "Regenerated");
  state = draftReducer(state, { type: "undo" });
  assert.equal(state.present["1:title"], undefined);
  assert.equal(state.manual["1:title"].value, null);
});
