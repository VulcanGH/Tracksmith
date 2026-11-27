import type { BatchRow, BatchView } from "../api/client";

export function batchStatusText(view: BatchView): string {
  const ready = view.rows.filter((row) => row.status === "ready").length;
  switch (view.phase) {
    case "planning":
      const checked = view.rows.filter((row) => !["queued", "reading", "analysing"].includes(row.status)).length;
      return view.total_files == null
        ? "Finding files…"
        : `Scanning files… ${checked} of ${view.total_files} checked · ${ready} ${ready === 1 ? "needs" : "need"} changes.`;
    case "review":
      if (!view.rows.length) return "Preview complete — no matching files found.";
      if (ready > 0)
        return `Preview complete — ${ready} ${ready === 1 ? "file" : "files"} ready to save.`;
      if (view.rows.some((row) => row.status !== "error" && row.warnings.length > 0))
        return "Preview complete — some files need review. Read the warnings.";
      return view.rows.some((row) => row.status === "error")
        ? "Preview complete — no files ready to save. Review the errors."
        : "Preview complete — no changes needed.";
    case "saving":
      return "Saving reviewed files…";
    case "finished":
      return "Batch finished.";
    case "cancelled":
      return "Batch stopped. Preview again to save remaining changes.";
    case "failed":
      return "Batch preview failed. Review the errors and preview again.";
    default:
      return view.phase;
  }
}

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
