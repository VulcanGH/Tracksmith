import { useQuery } from "@tanstack/react-query";
import { getBatchHistory } from "../api/client";
import { BatchRows } from "./batch-page";
export function ActivityPage() {
  const history = useQuery({
    queryKey: ["batch-history"],
    queryFn: getBatchHistory,
    refetchInterval: 5000,
  });
  return (
    <section className="details-panel">
      <h2>Recent batch reports</h2>
      <p className="muted">
        Up to 20 recent batches (32 MiB total). Per-file metadata records are saved in the
        configuration folder’s operations directory.
      </p>
      {history.error && <p role="alert">{history.error.message}</p>}
      {history.data?.length === 0 && <p>No completed batches yet.</p>}
      {history.data?.map((view) => (
        <details key={view.id}>
          <summary>
            {new Date(view.started_at * 1000).toLocaleString()} · {view.profile}{" "}
            · {view.phase} ·{" "}
            {view.rows.filter((r) => r.status === "verified").length}/
            {view.rows.length} verified
          </summary>
          {view.messages.map((m, i) => (
            <p key={i}>{m}</p>
          ))}
          <BatchRows view={view} />
        </details>
      ))}
    </section>
  );
}
