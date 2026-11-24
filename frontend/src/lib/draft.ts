import type { Edit } from "../api/client";

export type Draft = Record<string, Edit>;
export type Patch = { edit: Edit; original: unknown };
type Snapshot = {
  present: Draft;
  manual: Draft;
  proposals: Draft;
  originals: Record<string, unknown>;
};
export type DraftState = Snapshot & {
  past: Snapshot[];
  future: Snapshot[];
  group: string | null;
};
export type DraftAction =
  | { type: "apply" | "rules"; patches: Patch[]; group?: string }
  | { type: "release"; keys: string[] }
  | { type: "undo" | "redo" | "commit" | "reset" | "revert" };
export const emptyDraft = (): DraftState => ({
  present: {},
  manual: {},
  proposals: {},
  originals: {},
  past: [],
  future: [],
  group: null,
});
export const draftKey = (uid: string | null | undefined, property: string) =>
  `${uid ?? "container"}:${property}`;
function snapshot(state: DraftState): Snapshot {
  return {
    present: state.present,
    manual: state.manual,
    proposals: state.proposals,
    originals: state.originals,
  };
}
export function draftReducer(
  state: DraftState,
  action: DraftAction,
): DraftState {
  if (action.type === "reset") return emptyDraft();
  if (action.type === "commit") return { ...state, group: null };
  if (action.type === "undo") {
    if (!state.past.length) return state;
    return {
      ...state.past.at(-1)!,
      past: state.past.slice(0, -1),
      future: [snapshot(state), ...state.future],
      group: null,
    };
  }
  if (action.type === "redo") {
    if (!state.future.length) return state;
    return {
      ...state.future[0],
      past: [...state.past, snapshot(state)].slice(-60),
      future: state.future.slice(1),
      group: null,
    };
  }
  const next: Snapshot = {
    present: {},
    manual: { ...state.manual },
    proposals: { ...state.proposals },
    originals: { ...state.originals },
  };
  if (action.type === "revert") {
    next.manual = {};
    next.proposals = {};
  }
  if (action.type === "release")
    for (const key of action.keys) delete next.manual[key];
  if (action.type === "rules") next.proposals = {};
  if (action.type === "apply" || action.type === "rules")
    for (const { edit, original } of action.patches) {
      const key = draftKey(edit.track_uid, edit.property);
      next.originals[key] = original;
      if (action.type === "rules") next.proposals[key] = edit;
      else next.manual[key] = edit;
    }
  for (const [key, edit] of Object.entries({
    ...next.proposals,
    ...next.manual,
  })) {
    if (JSON.stringify(edit.value) !== JSON.stringify(next.originals[key]))
      next.present[key] = edit;
  }
  if (JSON.stringify(next) === JSON.stringify(snapshot(state))) return state;
  const group = action.type === "apply" ? (action.group ?? null) : null;
  return {
    ...next,
    past:
      group && group === state.group
        ? state.past
        : [...state.past, snapshot(state)].slice(-60),
    future: [],
    group,
  };
}

export function serializeTsv(rows: string[][]): string {
  return rows
    .map((row) =>
      row
        .map((cell) =>
          /[\t\n\r"]/.test(cell) ? `"${cell.replaceAll('"', '""')}"` : cell,
        )
        .join("\t"),
    )
    .join("\n");
}
/** A bounded two-column clipboard format for titles and languages, with quoted cells. */
export function parseTsv(input: string): string[][] {
  if (input.length > 1024 * 1024)
    throw new Error("Clipboard data exceeds 1 MiB.");
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  let closed = false;
  function finishCell() {
    row.push(cell);
    cell = "";
    closed = false;
    if (row.length > 2)
      throw new Error("Paste at most two columns: title and language.");
  }
  function finishRow() {
    finishCell();
    rows.push(row);
    row = [];
    if (rows.length > 1024) throw new Error("Paste at most 1,024 rows.");
  }
  for (let index = 0; index < input.length; index++) {
    const char = input[index];
    if (quoted) {
      if (char === '"') {
        if (input[index + 1] === '"') {
          cell += '"';
          index++;
        } else {
          quoted = false;
          closed = true;
        }
      } else cell += char;
    } else if (char === "\t") finishCell();
    else if (char === "\n" || char === "\r") {
      if (char === "\r" && input[index + 1] === "\n") index++;
      finishRow();
    } else if (char === '"' && !cell && !closed) quoted = true;
    else {
      if (closed)
        throw new Error("Unexpected text after a quoted clipboard cell.");
      cell += char;
    }
  }
  if (quoted) throw new Error("A quoted clipboard cell was not closed.");
  if (cell || row.length || !rows.length || closed) finishRow();
  return rows;
}
