import { test } from "node:test";
import assert from "node:assert/strict";
import { setImmediate } from "node:timers/promises";
import { createUpdateGuard } from "./update-lock.ts";

test("update restart excludes other tabs and blocks new tabs during preparation", async () => {
  const key = `tracksmith-test-${crypto.randomUUID()}`;
  const first = createUpdateGuard(navigator.locks, key);
  const second = createUpdateGuard(navigator.locks, key);
  const third = createUpdateGuard(navigator.locks, key);
  try {
    await first.hold();
    await second.hold();
    await assert.rejects(
      first.run(async () => "must not run"),
      /Close other Tracksmith tabs/,
    );
    await second.leave();
    let began!: () => void;
    let finish!: () => void;
    const started = new Promise<void>((done) => {
      began = done;
    });
    const waiting = new Promise<void>((done) => {
      finish = done;
    });
    const update = first.run(async () => {
      began();
      await waiting;
      return "verified";
    });
    await started;
    let entered = false;
    const newcomer = third.hold().then(() => {
      entered = true;
    });
    await setImmediate();
    assert.equal(entered, false);
    finish();
    assert.equal(await update, "verified");
    await newcomer;
    assert.equal(entered, true);
  } finally {
    await first.leave();
    await second.leave();
    await third.leave();
  }
});
