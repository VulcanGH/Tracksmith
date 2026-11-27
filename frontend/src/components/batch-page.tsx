import { useEffect, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button } from "./ui/button";
import {
  chooseBatchFile,
  chooseFolder,
  getBatch,
  getRuleLibrary,
  previewBatch,
  saveBatch,
  cancelBatch,
  clearBatch,
  recheckBatchFile,
  type BatchRow,
  getPending,
  folderPending,
  dismissPending,
  type BatchView,
} from "../api/client";
import { batchStatusText, filterBatchRows } from "../lib/batch";
import { prettyPath } from "../lib/paths";

export function BatchPage({ active, onOpenFile }: { active: boolean; onOpenFile?: (path: string) => Promise<void> }) {
  const jobClient = useQueryClient();
  const library = useQuery({
    queryKey: ["rule-library"],
    queryFn: getRuleLibrary,
  });
  const job = useQuery({
    queryKey: ["batch"],
    queryFn: getBatch,
    enabled: active,
    refetchInterval: (query) =>
      active && ["planning", "saving"].includes(query.state.data?.phase ?? "")
        ? 1000
        : false,
  });
  const pending = useQuery({
    queryKey: ["desktop-pending"],
    queryFn: getPending,
    enabled: active,
    refetchInterval: active ? 2000 : false,
  });
  const [sources, setSources] = useState<string[]>([]);
  const [profile, setProfile] = useState("");
  const [recursive, setRecursive] = useState(true);
  const [filter, setFilter] = useState("");
  const [search, setSearch] = useState("");
  const [skipUnchanged, setSkipUnchanged] = useState(true);
  const [excluded, setExcluded] = useState<Set<string>>(new Set());
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const view = job.data;
  const running = view?.phase === "planning" || view?.phase === "saving";
  async function run(task: () => Promise<unknown>) {
    setError("");
    setBusy(true);
    try {
      await task();
      await job.refetch();
      await pending.refetch();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Operation failed");
    } finally {
      setBusy(false);
    }
  }
  function add(path: string | null | undefined) {
    if (path)
      setSources((old) =>
        old.includes(path) ? old : [...old, path].slice(0, 32),
      );
  }
  const visibleRows = filterBatchRows(view?.rows ?? [], search, skipUnchanged);
  const selected =
    view?.rows
      .filter((r) => r.status === "ready" && !excluded.has(r.id))
      .map((r) => r.id) ?? [];
  const visibleIds = new Set(visibleRows.map((row) => row.id));
  const hiddenSelected = selected.filter((id) => !visibleIds.has(id)).length;
  return (
    <section className="batch-page">
      <div className="editor-toolbar">
        <Button
          disabled={busy || running || sources.length >= 32}
          onClick={() => void run(async () => add(await chooseBatchFile()))}
        >
          Add file
        </Button>
        <Button
          variant="outline"
          disabled={busy || running || sources.length >= 32}
          onClick={() =>
            void run(async () => add((await chooseFolder())?.path))
          }
        >
          Add folder
        </Button>
        <Button
          variant="outline"
          disabled={busy || running || (!sources.length && !view?.id)}
          onClick={() =>
            void run(async () => {
              await jobClient.cancelQueries({ queryKey: ["batch"] });
              const cleared = await clearBatch();
              jobClient.setQueryData(["batch"], cleared);
              setSources([]);
              setExcluded(new Set());
              setSearch("");
              setFilter("");
            })
          }
        >
          Clear
        </Button>
        <select
          aria-label="Batch profile"
          disabled={busy || running}
          value={profile || library.data?.active_profile_id || ""}
          onChange={(e) => setProfile(e.target.value)}
        >
          {library.data?.profiles.map((p) => (
            <option key={p.id} value={p.id}>
              {p.name}
            </option>
          ))}
        </select>
        <label>
          <input
            type="checkbox"
            checked={recursive}
            disabled={busy || running}
            onChange={(e) => setRecursive(e.target.checked)}
          />{" "}
          Subfolders
        </label>
        <label title="Hide unchanged files without warnings">
          <input
            type="checkbox"
            checked={skipUnchanged}
            onChange={(e) => setSkipUnchanged(e.target.checked)}
          />{" "}
          Skip unchanged
        </label>
        <input
          aria-label="Filename contains"
          placeholder="Filename contains…"
          value={filter}
          maxLength={256}
          disabled={busy || running}
          onChange={(e) => setFilter(e.target.value)}
        />
        <Button
          disabled={busy || running || !sources.length}
          onClick={() =>
            void run(async () => {
              setExcluded(new Set());
              await previewBatch({
                sources,
                recursive,
                filter,
                profile_id: profile || library.data?.active_profile_id || "",
              });
            })
          }
        >
          Preview batch
        </Button>
      </div>
      {pending.data
        ?.filter((p) => p.folder)
        .map((item) => (
          <div className="batch-source" key={item.id}>
            <span>Explorer: {prettyPath(item.path)}</span>
            <Button
              variant="outline"
              disabled={busy || running}
              onClick={() =>
                void run(async () => add((await folderPending(item.id)).path))
              }
            >
              Add folder
            </Button>
            <Button
              variant="ghost"
              onClick={() => void run(() => dismissPending(item.id))}
            >
              Dismiss
            </Button>
          </div>
        ))}
      {sources.map((path) => (
        <div className="batch-source" key={path}>
          <span title={path}>{prettyPath(path)}</span>
          <Button
            variant="ghost"
            disabled={busy || running}
            onClick={() => setSources((old) => old.filter((p) => p !== path))}
          >
            Remove
          </Button>
        </div>
      ))}
      <p className="muted">
        Files appear before checks finish. Track reading can run ahead of
        technical analysis; saving remains one file at a time. Preview uses a
        fixed profile revision. Writes modify originals in place; metadata
        records are not file backups.
      </p>
      {error && (
        <p role="alert" className="error-message">
          {error}
        </p>
      )}
      {job.error && <p role="alert">{job.error.message}</p>}
      {view?.id && (
        <>
          <div className="editor-toolbar">
            <strong className="batch-progress" role="status">
              {running && (
                <LoaderCircle className="batch-spinner" size={16} aria-hidden="true" />
              )}
              {batchStatusText(view)}
            </strong>
            <span>
              {view.profile} · revision {view.revision} · {view.rows.length}{" "}
              files · {(view.elapsed_ms / 1000).toFixed(1)}s
            </span>
            <span>
              {view.rows.filter((r) => r.status === "verified").length} verified
            </span>
            {running && (
              <Button variant="outline" onClick={() => void run(cancelBatch)}>
                Stop after current file
              </Button>
            )}
            <div className="batch-save-action">
              <Button
                disabled={busy || view.phase !== "review" || !selected.length}
                aria-describedby={view.phase === "planning" ? "batch-save-hint" : undefined}
                onClick={() => void run(() => saveBatch(view.id, selected))}
              >
                Save {selected.length} reviewed {selected.length === 1 ? "file" : "files"}
                {hiddenSelected > 0 && ` (${hiddenSelected} hidden)`}
              </Button>
              {view.phase === "planning" && (
                <span className="muted" id="batch-save-hint">
                  Available when scanning finishes.
                </span>
              )}
            </div>
            {!running && view.phase === "review" && (
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => void run(cancelBatch)}
              >
                Discard preview
              </Button>
            )}
          </div>
          <div className="editor-toolbar">
            <input
              aria-label="Search listed files"
              placeholder="Search listed files…"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            <span className="muted">
              {visibleRows.length} shown · {selected.length} selected
              {" · "}
              {view.rows.filter((r) => r.status === "ready").length} needing changes
              {" · "}
              {view.rows.filter((r) => r.status === "unchanged").length} unchanged
              {" · "}
              {view.rows.filter((r) => r.status === "error").length} failed
              {" · "}{view.rows.filter((r) => r.warnings.length > 0).length} with warnings
            </span>
          </div>
          {view.current && (
            <p className="muted" role="status">
              {prettyPath(view.current)}
            </p>
          )}
          {view.messages.map((m, i) => (
            <p role="alert" key={i}>
              {m}
            </p>
          ))}
          {view.rows.length > 0 && !visibleRows.length && (
            <p className="muted">No files match the current filters.</p>
          )}
          <BatchRows
            view={{ ...view, rows: visibleRows }}
            excluded={excluded}
            controlsDisabled={busy || running}
            recheck={view.phase === "review" ? (row, independent) => void run(() => recheckBatchFile(view.id, row.id, independent)) : undefined}
            openFile={onOpenFile ? (row) => void run(() => onOpenFile(row.path)) : undefined}
            toggle={(id) =>
              setExcluded((old) => {
                const next = new Set(old);
                if (next.has(id)) next.delete(id);
                else next.add(id);
                return next;
              })
            }
          />
        </>
      )}
    </section>
  );
}
export function BatchRows({
  view,
  excluded,
  toggle,
  recheck,
  openFile,
  controlsDisabled,
}: {
  view: BatchView;
  excluded?: Set<string>;
  toggle?: (id: string) => void;
  recheck?: (row: BatchRow, independent?: boolean) => void;
  openFile?: (row: BatchRow) => void;
  controlsDisabled?: boolean;
}) {
  return (
    <div className="batch-results">
      {view.rows.map((row) => (
        <BatchResultRow
          key={row.id}
          row={row}
          excluded={excluded?.has(row.id)}
          toggle={toggle}
          recheck={recheck}
          openFile={openFile}
          controlsDisabled={controlsDisabled}
        />
      ))}
    </div>
  );
}

function BatchResultRow({
  row,
  excluded,
  toggle,
  recheck,
  openFile,
  controlsDisabled,
}: {
  row: BatchRow;
  excluded?: boolean;
  toggle?: (id: string) => void;
  recheck?: (row: BatchRow, independent?: boolean) => void;
  openFile?: (row: BatchRow) => void;
  controlsDisabled?: boolean;
}) {
  const [open, setOpen] = useState(false);
  useEffect(() => { if (row.warnings.length > 0) setOpen(true); }, [row.warnings.length]);
  const checking = ["queued", "reading", "analysing"].includes(row.status);
  const labels: Record<string, string> = { queued: "Queued", reading: "Reading tracks…", analysing: "Checking technical details…", needs_review: "Needs review" };
  const status = row.status === "unchanged" && row.warnings.length ? "Needs review" : labels[row.status] ?? row.status;
  return (
    <details
      className="batch-row"
      open={open}
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary>
        {toggle && row.status === "ready" && (
          <input
            type="checkbox"
            aria-label={`Include ${prettyPath(row.path)}`}
            checked={!excluded}
            onClick={(e) => e.stopPropagation()}
            onChange={() => toggle(row.id)}
          />
        )}
        <span className="batch-status">{checking && <LoaderCircle className="batch-spinner" size={14} aria-hidden="true" />} {status}</span>
        <span title={row.path}>{prettyPath(row.path)}</span>
        <span className="muted">
          {checking ? "Not ready to save" : row.warnings.length && !row.changes.length ? "No changes proposed — review warnings" : `${row.changes.length} changes`}
          {row.warnings.length > 0 && ` · ${row.warnings.length} warnings`}
        </span>
      </summary>
      {open && row.warnings.length > 0 && (
        <div className="batch-warning-details" role="note">
          <strong>This file needs review</strong>
          {row.warnings.some((warning) => /Audio stream.*(identity|editable track)/i.test(warning)) && (
            <p>The audio check could not link every stream to an editable track. Those track names were left unchanged; this does not by itself indicate a damaged file.</p>
          )}
          {recheck || openFile ? <p>Verify independently uses full-file MediaInfo analysis to check this file again. Resolved warnings clear automatically; unresolved issues remain visible. This can take longer and does not write to the file. Check again repeats the normal check. Open in Editor lets you review or edit the tracks.</p> : <p>This saved report records the warning at the time of the scan. Open the file in Editor or preview it in a new batch to check its current state.</p>}
        </div>
      )}
      {open && (recheck || openFile) && (
        <div className="editor-toolbar">
          {recheck && <Button variant="outline" disabled={controlsDisabled} onClick={() => recheck(row)}>Check again</Button>}
          {recheck && (row.warnings.length > 0 || row.status === "error") && <Button variant="outline" disabled={controlsDisabled} onClick={() => recheck(row, true)}>Verify independently</Button>}
          {openFile && <Button variant="outline" disabled={controlsDisabled} onClick={() => openFile(row)}>Open in Editor</Button>}
        </div>
      )}
      {open && row.changes.length > 0 && (
        <table>
          <thead>
            <tr>
              <th>Target</th>
              <th>Property</th>
              <th>Before</th>
              <th>After</th>
            </tr>
          </thead>
          <tbody>
            {row.changes.map((c, i) => (
              <tr key={i}>
                <td>
                  {c.track_uid ? `Track ${c.track_uid}` : "Container"}
                </td>
                <td>{c.property}</td>
                <td>{JSON.stringify(c.before)}</td>
                <td>{JSON.stringify(c.after)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      {open &&
        row.warnings.map((warning, i) => (
          <p className="muted" key={i}>
            {warning}
          </p>
        ))}
      {open && row.operation_id && (
        <p className="muted">Metadata record: {row.operation_id}</p>
      )}
    </details>
  );
}
