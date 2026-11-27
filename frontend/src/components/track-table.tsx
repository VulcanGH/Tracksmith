import { Fragment, useState } from "react";
import type { ClipboardEvent, KeyboardEvent } from "react";
import {
  AudioLines,
  ChevronDown,
  FileQuestion,
  ListFilter,
  MessageSquareText,
  RotateCcw,
  Video,
  LockKeyhole,
} from "lucide-react";
import type { Edit, Inspection, Property, Track } from "../api/client";
import { draftKey, type Draft, type Patch } from "../lib/draft";
import { normalizeLanguageChoice } from "../lib/languages";
import { show, trackDetails, TrackTechnical } from "./media-details";
import { Button } from "./ui/button";

type Column =
  | "number"
  | "type"
  | "codec"
  | "details"
  | "title"
  | "language"
  | "default"
  | "forced"
  | "enabled";
const columns: { id: Column; label: string }[] = [
  { id: "number", label: "#" },
  { id: "type", label: "Type" },
  { id: "codec", label: "Format" },
  { id: "details", label: "Details" },
  { id: "title", label: "Track title" },
  { id: "language", label: "Language" },
  { id: "default", label: "Default" },
  { id: "forced", label: "Forced" },
  { id: "enabled", label: "Enabled" },
];
function readColumns(): Column[] {
  try {
    const value: unknown = JSON.parse(
      localStorage.getItem("tracksmith.columns") ?? "null",
    );
    if (Array.isArray(value))
      return columns
        .filter(
          (column) =>
            value.includes(column.id) ||
            ["number", "title"].includes(column.id),
        )
        .map((column) => column.id);
  } catch {
    /* Use defaults for old or invalid preferences. */
  }
  return columns.map((column) => column.id);
}
export function originalValue(
  doc: Inspection,
  uid: string | null | undefined,
  property: Property,
): unknown {
  if (!uid) return doc.title ?? null;
  const track = doc.tracks.find((track) => track.uid === uid);
  return property === "title"
    ? (track?.title ?? null)
    : property === "language"
      ? (track?.language_ietf ?? track?.language ?? null)
      : (track?.flags[property] ?? null);
}
export function makePatch(
  doc: Inspection,
  uid: string | null,
  property: Property,
  value: unknown,
): Patch {
  const original = originalValue(doc, uid, property);
  return {
    edit: {
      track_uid: uid,
      property,
      value:
        property === "language" &&
        typeof value === "string" &&
        value !== original
          ? normalizeLanguageChoice(value)
          : value,
    } as Edit,
    original,
  };
}

export function TrackTable({
  doc,
  draft,
  manual,
  proposals,
  onRelease,
  selected,
  busy,
  onSelection,
  onEdit,
  onCommit,
  onPaste,
  onRevert,
}: {
  doc: Inspection;
  draft: Draft;
  manual: Draft;
  proposals: Draft;
  onRelease: (uid: string | null, property: Property) => void;
  selected: Set<string>;
  busy: boolean;
  onSelection: (uids: Set<string>) => void;
  onEdit: (patches: Patch[], group?: string) => void;
  onCommit: () => void;
  onPaste: (
    event: ClipboardEvent<HTMLInputElement>,
    track: Track,
    property: "title" | "language",
  ) => void;
  onRevert: (uid: string) => void;
}) {
  const [visible, setVisible] = useState<Column[]>(readColumns);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [anchor, setAnchor] = useState(0);
  const activeColumns = columns.filter((column) => visible.includes(column.id));
  const changed = (uid: string, property: Property) =>
    !!draft[draftKey(uid, property)];
  function current(track: Track, property: Property) {
    const patch = draft[draftKey(track.uid, property)];
    return patch ? patch.value : originalValue(doc, track.uid, property);
  }
  function source(track: Track, property: Property) {
    const key = draftKey(track.uid, property);
    return manual[key] ? "manual" : proposals[key] ? "rules" : "original";
  }
  function release(track: Track, property: Property) {
    return manual[draftKey(track.uid, property)] ? (
      <button
        className="cell-release"
        disabled={busy}
        title="Release manual override; apply rules to refresh"
        aria-label={`Release ${property} override, track ${track.number}`}
        onClick={() => onRelease(track.uid, property)}
      >
        <LockKeyhole size={12} />
      </button>
    ) : null;
  }
  function edit(
    track: Track,
    property: Property,
    value: unknown,
    group?: string,
  ) {
    onEdit([makePatch(doc, track.uid, property, value)], group);
  }
  function navigation(
    event: KeyboardEvent<HTMLInputElement>,
    property: Property,
  ) {
    if (
      event.ctrlKey ||
      event.metaKey ||
      !["ArrowUp", "ArrowDown", "Enter"].includes(event.key)
    )
      return;
    // Leave ordinary language arrows/Enter to the browser's searchable datalist.
    if (property === "language" && !event.altKey) return;
    event.preventDefault();
    onCommit();
    const inputs = Array.from(
      event.currentTarget
        .closest("table")
        ?.querySelectorAll<HTMLInputElement>(
          `input[data-field="${property}"]:not(:disabled)`,
        ) ?? [],
    );
    const index = inputs.indexOf(event.currentTarget);
    const next =
      inputs[
        index +
          (event.key === "ArrowUp" || (event.key === "Enter" && event.shiftKey)
            ? -1
            : 1)
      ];
    next?.focus();
    if (next?.type === "text") next.select();
  }
  function flag(track: Track, property: Property, label: string) {
    const value = current(track, property);
    const editable = track.editable.includes(property);
    return (
      <label
        data-source={source(track, property)}
        className={`flag-control ${changed(track.uid, property) ? "modified" : ""}`}
        title={`${label} · Original: ${show(originalValue(doc, track.uid, property))}${!editable ? " · Read-only or unsupported" : ""}`}
      >
        <input
          type="checkbox"
          data-field={property}
          aria-label={`${label}, track ${track.number}`}
          aria-checked={value == null ? "mixed" : Boolean(value)}
          ref={(node) => {
            if (node) node.indeterminate = value == null;
          }}
          checked={Boolean(value)}
          disabled={busy || !editable}
          onKeyDown={(event) => navigation(event, property)}
          onChange={(event) => edit(track, property, event.target.checked)}
        />
        <span className="extra-flag-label">{label}</span>
      </label>
    );
  }
  function cell(track: Track, column: Column) {
    if (column === "number") return track.number;
    if (column === "type") {
      const Icon =
        track.kind === "video"
          ? Video
          : track.kind === "audio"
            ? AudioLines
            : track.kind === "subtitles"
              ? MessageSquareText
              : FileQuestion;
      return (
        <span className="track-type" title={track.kind}>
          <Icon size={15} />
          <span className="sr-only">{track.kind}</span>
        </span>
      );
    }
    if (column === "codec")
      return (
        <span className="codec-name" title={track.codec}>
          {track.codec}
        </span>
      );
    if (column === "details")
      return <span title={trackDetails(track)}>{trackDetails(track)}</span>;
    if (column === "title" || column === "language")
      return (
        <div className="cell-editor">
          <input
            type="text"
            data-source={source(track, column)}
            data-field={column}
            className={`field inline-field ${column === "language" ? "language-field" : ""} ${changed(track.uid, column) ? "modified" : ""}`}
            aria-label={`${column}, track ${track.number}`}
            value={String(current(track, column) ?? "")}
            placeholder={column === "title" ? "Absent" : "Undefined"}
            title={
              column === "language"
                ? `Legacy: ${show(track.language)} ? IETF: ${show(track.language_ietf)}`
                : `Original: ${show(originalValue(doc, track.uid, column))}`
            }
            list={column === "language" ? "language-tags" : undefined}
            disabled={busy || !track.editable.includes(column)}
            onBlur={onCommit}
            onKeyDown={(event) => navigation(event, column)}
            onPaste={(event) => onPaste(event, track, column)}
            onChange={(event) =>
              edit(
                track,
                column,
                column === "title"
                  ? event.target.value || null
                  : event.target.value,
                draftKey(track.uid, column),
              )
            }
          />
          {changed(track.uid, column) && (
            <button
              className="cell-revert"
              disabled={busy}
              aria-label={`Revert ${column}, track ${track.number}`}
              title="Revert this property"
              onClick={() =>
                edit(track, column, originalValue(doc, track.uid, column))
              }
            >
              <RotateCcw size={12} />
            </button>
          )}
          {release(track, column)}
        </div>
      );
    return (
      <span className="flag-cell">
        {flag(track, column, columns.find((c) => c.id === column)!.label)}
        {changed(track.uid, column) && (
          <button
            className="cell-revert"
            disabled={busy}
            title="Revert this property"
            aria-label={`Revert ${column}, track ${track.number}`}
            onClick={() =>
              edit(track, column, originalValue(doc, track.uid, column))
            }
          >
            <RotateCcw size={12} />
          </button>
        )}
        {release(track, column)}
      </span>
    );
  }
  return (
    <>
      <div className="table-preferences">
        <span>
          {selected.size} selected · Tab / ↑ ↓ to navigate · Paste
          title/language columns
        </span>
        <details className="column-options">
          <summary>
            <ListFilter size={13} />
            Columns
          </summary>
          <div>
            {columns.map((column) => (
              <label key={column.id}>
                <input
                  type="checkbox"
                  checked={visible.includes(column.id)}
                  disabled={["number", "title"].includes(column.id)}
                  onChange={(event) => {
                    const next = event.target.checked
                      ? [...visible, column.id]
                      : visible.filter((id) => id !== column.id);
                    setVisible(next);
                    localStorage.setItem(
                      "tracksmith.columns",
                      JSON.stringify(next),
                    );
                  }}
                />
                {column.label}
              </label>
            ))}
          </div>
        </details>
      </div>
      <div className="track-table-scroll">
        <table className="track-table compact-tracks">
          <thead>
            <tr>
              <th className="select-column">
                <input
                  type="checkbox"
                  aria-label="Select all tracks"
                  checked={
                    selected.size === doc.tracks.length && doc.tracks.length > 0
                  }
                  ref={(node) => {
                    if (node)
                      node.indeterminate =
                        selected.size > 0 && selected.size < doc.tracks.length;
                  }}
                  disabled={busy}
                  onChange={(event) =>
                    onSelection(
                      new Set(
                        event.target.checked
                          ? doc.tracks.map((track) => track.uid)
                          : [],
                      ),
                    )
                  }
                />
              </th>
              {activeColumns.map((column) => (
                <th
                  className={`column-${column.id}`}
                  key={column.id}
                  title={column.label}
                >
                  {column.label}
                </th>
              ))}
              <th className="expand-column">
                <span className="sr-only">More</span>
              </th>
            </tr>
          </thead>
          <tbody>
            {doc.tracks.map((track, index) => (
              <Fragment key={track.uid}>
                <tr data-selected={selected.has(track.uid)}>
                  <td className="select-column">
                    <input
                      type="checkbox"
                      aria-label={`Select track ${track.number}`}
                      checked={selected.has(track.uid)}
                      disabled={busy}
                      onChange={(event) => {
                        const next = new Set(selected);
                        if ((event.nativeEvent as MouseEvent).shiftKey) {
                          for (const item of doc.tracks.slice(
                            Math.min(anchor, index),
                            Math.max(anchor, index) + 1,
                          ))
                            next.add(item.uid);
                        } else {
                          setAnchor(index);
                          if (event.target.checked) next.add(track.uid);
                          else next.delete(track.uid);
                        }
                        onSelection(next);
                      }}
                    />
                  </td>
                  {activeColumns.map((column) => (
                    <td key={column.id} className={`column-${column.id}`}>
                      {cell(track, column.id)}
                    </td>
                  ))}
                  <td className="expand-column">
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={`More information, track ${track.number}`}
                      aria-expanded={expanded.has(track.uid)}
                      onClick={() =>
                        setExpanded((previous) => {
                          const next = new Set(previous);
                          if (next.has(track.uid)) next.delete(track.uid);
                          else next.add(track.uid);
                          return next;
                        })
                      }
                    >
                      <ChevronDown />
                    </Button>
                  </td>
                </tr>
                {expanded.has(track.uid) && (
                  <tr className="expanded-track">
                    <td colSpan={activeColumns.length + 2}>
                      <div className="expanded-heading">
                        <code>UID {track.uid}</code>
                        <span>
                          mkvmerge ID {track.id} · Technical mapping:{" "}
                          {track.mapping === "uid_number_type_verified"
                            ? "verified mapping"
                            : "unconfirmed mapping"}
                        </span>
                        <Button
                          variant="ghost"
                          disabled={
                            busy ||
                            !Object.values(draft).some(
                              (edit) => edit.track_uid === track.uid,
                            )
                          }
                          onClick={() => onRevert(track.uid)}
                        >
                          <RotateCcw />
                          Revert track
                        </Button>
                      </div>
                      <div className="extra-flags">
                        {(
                          [
                            "hearing_impaired",
                            "commentary",
                            "original",
                            "visual_impaired",
                            "text_descriptions",
                          ] as Property[]
                        ).map((property) => (
                          <span key={property}>
                            {flag(
                              track,
                              property,
                              property.replaceAll("_", " "),
                            )}
                            {changed(track.uid, property) && (
                              <button
                                className="cell-revert"
                                disabled={busy}
                                title="Revert this property"
                                aria-label={`Revert ${property}, track ${track.number}`}
                                onClick={() =>
                                  edit(
                                    track,
                                    property,
                                    originalValue(doc, track.uid, property),
                                  )
                                }
                              >
                                <RotateCcw size={12} />
                              </button>
                            )}
                            {release(track, property)}
                          </span>
                        ))}
                      </div>
                      <TrackTechnical track={track} />
                    </td>
                  </tr>
                )}
              </Fragment>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
