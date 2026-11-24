import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  checkUpdates,
  getUpdates,
  installUpdate,
  getBatch,
  launchUpdateCheck,
  type UpdateStatus,
} from "../api/client";
import { Button } from "./ui/button";
import { withUpdateTabLock } from "../lib/update-lock";

export function UpdatesPanel({ draftsClear }: { draftsClear: boolean }) {
  const query = useQuery({
    queryKey: ["updates"],
    queryFn: getUpdates,
    refetchInterval: 1500,
    retry: false,
  });
  const batch = useQuery({
    queryKey: ["batch"],
    queryFn: getBatch,
    refetchInterval: 1500,
  });
  const client = useQueryClient();
  const [busy, setBusy] = useState(false);
  const [installing, setInstalling] = useState(false);
  const [error, setError] = useState("");
  const started = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const data = query.data;
  const restarting = data?.phase === "restarting";
  const blocked =
    batch.data?.phase === "planning" ||
    batch.data?.phase === "saving" ||
    (batch.data?.phase === "review" &&
      batch.data.rows.some((r) => r.status === "ready"));
  const safe = draftsClear && !!batch.data && !blocked;
  function update(value: UpdateStatus) {
    client.setQueryData(["updates"], value);
  }
  async function check() {
    setBusy(true);
    setError("");
    try {
      update(await checkUpdates());
    } catch (e) {
      setError(e instanceof Error ? e.message : "Update check failed");
      await query.refetch();
    } finally {
      setBusy(false);
    }
  }
  async function install() {
    if (!safe || !data?.available) return;
    setInstalling(true);
    setError("");
    try {
      const version = data.available;
      update(await withUpdateTabLock(() => installUpdate(version)));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Update preparation failed");
      setInstalling(false);
      await query.refetch();
    }
  }
  useEffect(() => {
    if (launchUpdateCheck && !started.current) {
      started.current = true;
      void check();
    }
  }, []);
  useEffect(() => {
    const modal = dialog.current;
    if (!modal) return;
    if ((installing || restarting) && !modal.open) modal.showModal();
    else if (!installing && !restarting && modal.open) modal.close();
  }, [installing, restarting]);
  return (
    <section className="updates-panel">
      <h2 className="tools-heading">Updates</h2>
      <div className="editor-toolbar">
        <span>Installed: {data?.current ?? "…"}</span>
        <Button
          variant="outline"
          disabled={busy || installing || data?.phase === "checking"}
          onClick={() => void check()}
        >
          Check for updates
        </Button>
        {data?.available && (
          <Button
            disabled={!safe || installing || busy}
            onClick={() => void install()}
          >
            {data.kind === "installed" ? "Install" : "Update"} {data.available}{" "}
            and restart
          </Button>
        )}
      </div>
      <p role="status" className="muted">
        {data?.message ?? "Checks stable GitHub Releases on request."}
      </p>
      {!safe && data?.available && (
        <p className="muted">
          Save or revert editor/rule drafts, finish or discard the batch review,
          and open or dismiss Explorer selections before updating.
        </p>
      )}
      {data?.kind === "installed" && (
        <p className="muted">
          Windows requests administrator approval to update the installed copy.
        </p>
      )}
      {data?.last_result && <p role="status">{data.last_result}</p>}
      {(error || query.error) && (
        <p role="alert" className="form-error">
          {error || query.error?.message}
        </p>
      )}
      {data?.notes && (
        <details>
          <summary>Release notes</summary>
          <pre className="update-notes">{data.notes}</pre>
        </details>
      )}
      {data?.release_url && (
        <p>
          <a href={data.release_url} target="_blank" rel="noreferrer">
            View release on GitHub
          </a>
        </p>
      )}
      <dialog
        ref={dialog}
        className="update-progress"
        onCancel={(e) => e.preventDefault()}
        aria-labelledby="update-heading"
      >
        <h2 id="update-heading">
          {restarting ? "Restarting Tracksmith" : "Preparing update"}
        </h2>
        <p role="status">
          {data?.message || "Downloading and verifying the update…"}
        </p>
        <p className="muted">
          {restarting
            ? "A new browser tab opens after the update. You can close this tab then."
            : "Your media files are not being changed."}
        </p>
      </dialog>
    </section>
  );
}
