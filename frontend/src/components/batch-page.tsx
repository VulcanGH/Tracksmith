import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Button } from "./ui/button";
import {
  chooseBatchFile,
  chooseFolder,
  getBatch,
  getRuleLibrary,
  previewBatch,
  saveBatch,
  cancelBatch,
  getPending,
  folderPending,
  dismissPending,
  type BatchView,
} from "../api/client";
import { prettyPath } from "../lib/paths";

export function BatchPage({ active }: { active: boolean }) {
  const library = useQuery({
    queryKey: ["rule-library"],
    queryFn: getRuleLibrary,
  });
  const job = useQuery({
    queryKey: ["batch"],
    queryFn: getBatch,
    enabled: active,
    refetchInterval: active ? 1000 : false,
  });
  const pending = useQuery({
    queryKey: ["desktop-pending"],
    queryFn: getPending,
    enabled: active,
    refetchInterval: active ? 2000 : false,
  });
  const [sources, setSources] = useState<string[]>([]);
  const [profile, setProfile] = useState("");
  const [recursive, setRecursive] = useState(false);
  const [filter, setFilter] = useState("");
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
  const selected =
    view?.rows
      .filter((r) => r.status === "ready" && !excluded.has(r.id))
      .map((r) => r.id) ?? [];
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
        Files are processed one at a time. Preview uses a fixed profile
        revision. Writes modify originals in place; metadata records are not
        file backups.
      </p>
      {error && (
        <p role="alert" className="error-message">
          {error}
        </p>
      )}
      {job.error && <p role="alert">{job.error.message}</p>}
      {view?.id && (
        <>
          <div className="editor-toolbar" role="status">
            <strong>{view.phase}</strong>
            <span>
              {view.profile} · revision {view.revision} · {view.rows.length}{" "}
              files · {(view.elapsed_ms / 1000).toFixed(1)}s
            </span>
            <span>
              {view.rows.filter((r) => r.status === "verified").length} verified
            </span>
            {running ? (
              <Button variant="outline" onClick={() => void run(cancelBatch)}>
                Stop after current file
              </Button>
            ) : (
              view.phase === "review" && (
                <Button
                  disabled={busy || !selected.length}
                  onClick={() => void run(() => saveBatch(view.id, selected))}
                >
                  Save {selected.length} reviewed files
                </Button>
              )
            )}
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
          <BatchRows
            view={view}
            excluded={excluded}
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
}: {
  view: BatchView;
  excluded?: Set<string>;
  toggle?: (id: string) => void;
}) {
  return (
    <div className="batch-results">
      {view.rows.map((row) => (
        <details className="batch-row" key={row.id}>
          <summary>
            {toggle && row.status === "ready" && (
              <input
                type="checkbox"
                aria-label={`Include ${prettyPath(row.path)}`}
                checked={!excluded?.has(row.id)}
                onClick={(e) => e.stopPropagation()}
                onChange={() => toggle(row.id)}
              />
            )}
            <span className="batch-status">{row.status}</span>
            <span title={row.path}>{prettyPath(row.path)}</span>
            <span className="muted">{row.changes.length} changes</span>
          </summary>
          {row.changes.length > 0 && (
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
          {row.warnings.map((warning, i) => (
            <p className="muted" key={i}>
              {warning}
            </p>
          ))}
          {row.operation_id && (
            <p className="muted">Metadata record: {row.operation_id}</p>
          )}
        </details>
      ))}
    </div>
  );
}
