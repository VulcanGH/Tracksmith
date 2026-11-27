import assert from "node:assert/strict";
import { test } from "node:test";
import { batchStatusText, filterBatchRows } from "./batch.ts";
import type { BatchRow, BatchView } from "../api/client.ts";

const row = (id: string, status: string, warnings: string[] = []): BatchRow => ({
  id,
  path: `C:\\Films\\${id}.mkv`,
  status,
  warnings,
  changes: [],
  operation_id: null,
});

const view = (phase: string, rows: BatchRow[] = []): BatchView => ({
  id: "batch", phase, rows, total_files: null, profile: "Standard",
  revision: 0, current: "", messages: [], elapsed_ms: 0, started_at: 0,
});

test("discovery and inspection explain that a populated list is still scanning", () => {
  assert.equal(batchStatusText(view("planning")), "Finding files…");
  const scanning = view("planning", [row("fix", "ready"), row("ok", "unchanged")]);
  scanning.total_files = 456;
  assert.equal(batchStatusText(scanning),
    "Scanning files… 2 of 456 checked · 1 needs changes.");
  scanning.rows.push(row("queued", "queued"), row("reading", "reading"), row("analysing", "analysing"));
  assert.equal(batchStatusText(scanning),
    "Scanning files… 2 of 456 checked · 1 needs changes.");
});

test("completion distinguishes ready files, unchanged files, empty scans and errors", () => {
  assert.equal(batchStatusText(view("review", [row("fix", "ready")])),
    "Preview complete — 1 file ready to save.");
  assert.equal(batchStatusText(view("review", [row("ok", "unchanged")])),
    "Preview complete — no changes needed.");
  assert.equal(batchStatusText(view("review")),
    "Preview complete — no matching files found.");
  assert.equal(batchStatusText(view("review", [row("bad", "error")])),
    "Preview complete — no files ready to save. Review the errors.");
  assert.match(batchStatusText(view("saving")), /^Saving/);
  assert.match(batchStatusText(view("cancelled")), /Preview again/);
  assert.match(batchStatusText(view("failed")), /failed/);
});

test("skip unchanged retains errors and warning-bearing files", () => {
  const rows = [
    row("ready", "ready"),
    row("clean", "unchanged"),
    row("warning", "unchanged", ["Inspect this"]),
    row("failed", "error"),
    row("pending", "queued"),
  ];
  assert.deepEqual(
    filterBatchRows(rows, "", true).map((r) => r.id),
    ["ready", "warning", "failed", "pending"],
  );
  assert.deepEqual(filterBatchRows(rows, "", false), rows);
});

test("search matches filenames and paths without changing underlying rows", () => {
  const rows = [row("Amelie", "ready"), row("Other", "ready")];
  assert.deepEqual(filterBatchRows(rows, " AMELIE ", true), [rows[0]]);
  assert.deepEqual(filterBatchRows(rows, "films", true), rows);
  assert.deepEqual(filterBatchRows(rows, "absent", true), []);
  assert.equal(rows.length, 2);
});
