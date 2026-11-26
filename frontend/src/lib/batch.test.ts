import assert from "node:assert/strict";
import { test } from "node:test";
import { filterBatchRows } from "./batch.ts";
import type { BatchRow } from "../api/client.ts";

const row = (id: string, status: string, warnings: string[] = []): BatchRow => ({
  id,
  path: `C:\\Films\\${id}.mkv`,
  status,
  warnings,
  changes: [],
  operation_id: null,
});

test("skip unchanged retains errors and warning-bearing files", () => {
  const rows = [
    row("ready", "ready"),
    row("clean", "unchanged"),
    row("warning", "unchanged", ["Inspect this"]),
    row("failed", "error"),
  ];
  assert.deepEqual(
    filterBatchRows(rows, "", true).map((r) => r.id),
    ["ready", "warning", "failed"],
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
