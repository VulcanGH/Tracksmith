import type { Inspection, RulePreview } from "../api/client";
import { show } from "./media-details";

export function RulePreviewPanel({
  result,
  doc,
}: {
  result: RulePreview;
  doc?: Inspection;
}) {
  const target = (uid: string | null | undefined) =>
    uid
      ? `Track ${doc?.tracks.find((track) => track.uid === uid)?.number ?? uid}`
      : "Container";
  return (
    <section className="rule-preview-panel">
      <header>
        <h3>Rule preview</h3>
        <span>
          {result.proposals.length} proposed properties · {result.passes} passes
          · {result.preserved_manual} overrides protected
        </span>
      </header>
      {result.proposals.length ? (
        <div className="rule-results-scroll">
          <table className="component-table">
            <thead>
              <tr>
                <th>Target</th>
                <th>Property</th>
                <th>Original</th>
                <th>Proposed</th>
                <th>Rule</th>
              </tr>
            </thead>
            <tbody>
              {result.proposals.map((proposal, index) => (
                <tr key={index}>
                  <td>{target(proposal.edit.track_uid)}</td>
                  <td>{proposal.edit.property.replaceAll("_", " ")}</td>
                  <td>{show(proposal.before)}</td>
                  <td>{show(proposal.edit.value)}</td>
                  <td title={proposal.rule_id}>{proposal.rule_name}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        <p className="muted">No metadata changes proposed.</p>
      )}
      {result.conflicts.length > 0 && (
        <details open>
          <summary>
            Conflicts encountered while settling ({result.conflicts.length})
          </summary>
          <table className="component-table">
            <thead>
              <tr>
                <th>Pass / target</th>
                <th>Property</th>
                <th>Priority winner</th>
                <th>Ignored match</th>
              </tr>
            </thead>
            <tbody>
              {result.conflicts.map((conflict, index) => (
                <tr key={index}>
                  <td>
                    {conflict.pass} · {target(conflict.target.track_uid)}
                  </td>
                  <td>{conflict.target.property.replaceAll("_", " ")}</td>
                  <td title={conflict.winner_id}>
                    {conflict.winner}
                    <small>{show(conflict.winning_value)}</small>
                  </td>
                  <td title={conflict.ignored_id}>
                    {conflict.ignored}
                    <small>{show(conflict.ignored_value)}</small>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
      {result.notes.length > 0 && (
        <details>
          <summary>Evaluation notes ({result.notes.length})</summary>
          <ul>
            {result.notes.map((note, index) => (
              <li key={index}>{note}</li>
            ))}
          </ul>
        </details>
      )}
    </section>
  );
}
