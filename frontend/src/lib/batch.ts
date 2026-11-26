import type { BatchRow } from "../api/client";

export function filterBatchRows(
  rows: BatchRow[],
  search: string,
  skipUnchanged: boolean,
): BatchRow[] {
  const query = search.trim().toLowerCase();
  return rows.filter((row) =>
    (!skipUnchanged || row.status !== "unchanged" || row.warnings.length > 0) &&
    row.path.toLowerCase().includes(query),
  );
}
