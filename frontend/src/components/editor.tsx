import { useEffect, useReducer, useRef, useState } from "react";
import type { ClipboardEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Check,
  FileVideo,
  Folder,
  FolderOpen,
  Redo2,
  RefreshCw,
  RotateCcw,
  Save,
  ShieldCheck,
  Undo2,
  X,
  Sparkles,
  LockKeyhole,
} from "lucide-react";
import {
  chooseFile,
  inspectFile,
  getEnrichment,
  cancelEnrichment,
  openRecent,
  previewChanges,
  restorePlan,
  savePlan,
  type Inspection,
  type Plan,
  type Property,
  type SaveReport,
  type Track,
  type Edit,
  type RulePreview,
  getRuleLibrary,
  previewRules,
  saveRuleLibrary,
  getPending,
  openPending,
  dismissPending,
} from "../api/client";
import {
  draftKey,
  draftReducer,
  emptyDraft,
  parseTsv,
  serializeTsv,
  type Patch,
} from "../lib/draft";
import { languages } from "../lib/languages";
import { prettyPath } from "../lib/paths";
import {
  ContainerSection,
  duration,
  sectionCount,
  show,
  size,
} from "./media-details";
import { TrackTable, makePatch, originalValue } from "./track-table";
import { FileBrowser } from "./file-browser";
import { Button } from "./ui/button";
import { RulePreviewPanel } from "./rule-preview";

export function Editor({
  hasLocations,
  active = true,
  onActivity,
  onInspection,
  onManualEdits,
  onDirtyChange,
}: {
  hasLocations: boolean;
  active?: boolean;
  onActivity: (message: string) => void;
  onInspection?: (inspection: Inspection | undefined) => void;
  onManualEdits?: (edits: Edit[]) => void;
  onDirtyChange?: (dirty: boolean) => void;
}) {
  const [path, setPath] = useState("");
  const pending = useQuery({
    queryKey: ["desktop-pending"],
    queryFn: getPending,
    enabled: active,
    refetchInterval: active ? 2000 : false,
  });
  const [doc, setDoc] = useState<Inspection>();
  const currentDoc = useRef(doc);
  currentDoc.current = doc;
  const [history, dispatch] = useReducer(draftReducer, undefined, emptyDraft);
  const draft = history.present;
  const [plan, setPlan] = useState<Plan>();
  const [report, setReport] = useState<SaveReport>();
  const [busy, setBusy] = useState("");
  const operation = useRef(false);
  const [error, setError] = useState("");
  const [tab, setTab] = useState("Tracks");
  const [browser, setBrowser] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [bulkProperty, setBulkProperty] = useState<Property>("language");
  const [bulkValue, setBulkValue] = useState("");
  const libraryQuery = useQuery({
    queryKey: ["rule-library"],
    queryFn: getRuleLibrary,
    staleTime: 30000,
  });
  const queryClient = useQueryClient();
  const libraryRevision = useRef<number | null>(null);
  const [ruleReport, setRuleReport] = useState<RulePreview>();
  const activeProfile = libraryQuery.data?.profiles.find(
    (profile) => profile.id === libraryQuery.data.active_profile_id,
  );
  const manualSignature = JSON.stringify(history.manual);
  const ruleContext = useRef("");
  ruleContext.current = JSON.stringify([doc?.id, doc?.analysis?.status, manualSignature, libraryQuery.data?.revision]);
  const stagedPhases = useRef(new Set<string>());
  const autoInspection = useRef<string | undefined>(undefined);
  const enrichment = useQuery({
    queryKey: ["enrichment", doc?.id],
    queryFn: () => getEnrichment(doc!.id),
    enabled: !!doc && (doc.analysis?.status === "pending" || doc.sections_pending),
    refetchInterval: (query) => query.state.error ? false : 500,
    retry: false,
  });
  useEffect(() => {
    const id = doc?.id;
    return () => { if (id) void cancelEnrichment(id).catch(() => {}); };
  }, [doc?.id]);
  useEffect(() => {
    const next = enrichment.data;
    if (!next || next.id !== currentDoc.current?.id || next.analysis?.status === "pending" || next.sections_pending) return;
    setDoc(next);
    onInspection?.(next);
  }, [enrichment.data, onInspection]);
  useEffect(() => {
    if (!doc || libraryQuery.data?.revision === undefined) return;
    let cancelled = false;
    void getEnrichment(doc.id).then((next) => {
      const current = currentDoc.current;
      if (cancelled || current?.id !== next.id) return;
      if (current.analysis?.status !== next.analysis?.status || current.analysis?.scope !== next.analysis?.scope || current.sections_pending !== next.sections_pending) {
        setDoc(next);
        onInspection?.(next);
      }
    }).catch(() => {});
    return () => { cancelled = true; };
  }, [doc?.id, libraryQuery.data?.revision, onInspection]);
  useEffect(() => {
    if (!doc || autoInspection.current !== doc.id || !activeProfile?.auto_apply || busy) return;
    const key = `${doc.id}:${doc.analysis?.status === "pending" ? "container" : "technical"}`;
    if (stagedPhases.current.has(key)) return;
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void stageRules(doc, activeProfile.id, Object.values(history.manual), () => !cancelled)
        .then((applied) => { if (applied) stagedPhases.current.add(key); })
        .catch((error: unknown) => {
          if (!cancelled) setError(error instanceof Error ? error.message : "Automatic rules are unavailable");
        });
    }, 150);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [doc?.id, doc?.analysis?.status, manualSignature, libraryQuery.data?.revision, busy]);
  useEffect(() => {
    onManualEdits?.(Object.values(history.manual));
  }, [history.manual, onManualEdits]);
  useEffect(() => {
    const revision = libraryQuery.data?.revision;
    if (revision === undefined) return;
    if (
      libraryRevision.current !== null &&
      revision !== libraryRevision.current
    ) {
      dispatch({ type: "rules", patches: [] });
      stagedPhases.current.clear();
      setRuleReport(undefined);
      setPlan(undefined);
    }
    libraryRevision.current = revision;
  }, [libraryQuery.data?.revision]);
  const changes = Object.values(draft);
  const dirty = changes.length > 0;
  useEffect(() => {
    onDirtyChange?.(
      dirty || Object.keys(history.manual).length > 0 || !!busy || browser,
    );
  }, [dirty, history.manual, busy, browser, onDirtyChange]);
  const modifiedTracks = new Set(
    changes.map((change) => change.track_uid).filter(Boolean),
  ).size;
  async function run(label: string, task: () => Promise<void>) {
    if (operation.current) return;
    operation.current = true;
    setBusy(label);
    setError("");
    onActivity(label);
    try {
      await task();
    } catch (error) {
      setError(error instanceof Error ? error.message : "Operation failed");
      onActivity("Operation failed");
    } finally {
      operation.current = false;
      setBusy("");
    }
  }
  function apply(patches: Patch[], group?: string) {
    setRuleReport(undefined);
    setPlan(undefined);
    setReport(undefined);
    setError("");
    dispatch({ type: "apply", patches, group });
  }
  function undo() {
    autoInspection.current = undefined;
    setRuleReport(undefined);
    setPlan(undefined);
    setError("");
    dispatch({ type: "undo" });
  }
  function redo() {
    setRuleReport(undefined);
    setPlan(undefined);
    setError("");
    dispatch({ type: "redo" });
  }
  async function showInspection(inspected: Inspection) {
    autoInspection.current = inspected.id;
    stagedPhases.current.clear();
    setPath(prettyPath(inspected.path));
    setDoc(inspected);
    dispatch({ type: "reset" });
    setPlan(undefined);
    setReport(undefined);
    setSelected(new Set());
    setTab("Tracks");
    setRuleReport(undefined);
    onInspection?.(inspected);
    onActivity(
      `Tracks loaded in ${inspected.elapsed_ms} ms · ${inspected.write_available ? "In-place editing available" : "Read-only"}`,
    );
    try {
      const library = await getRuleLibrary();
      libraryRevision.current = library.revision;
      queryClient.setQueryData(["rule-library"], library);
    } catch (error) {
      setError(
        `File inspected; automatic rules are unavailable: ${error instanceof Error ? error.message : "Unknown error"}`,
      );
    }
  }
  async function stageRules(
    inspection: Inspection,
    profileId: string,
    manual = Object.values(history.manual),
    stillCurrent = () => true,
  ) {
    const context = ruleContext.current;
    const result = await previewRules(inspection.id, profileId, manual);
    if (!stillCurrent() || currentDoc.current?.id !== inspection.id || context !== ruleContext.current) return false;
    dispatch({
      type: "rules",
      patches: result.proposals.map((proposal) =>
        makePatch(
          inspection,
          proposal.edit.track_uid ?? null,
          proposal.edit.property,
          proposal.edit.value,
        ),
      ),
    });
    setRuleReport(result);
    setPlan(undefined);
    onActivity(
      `${result.proposals.length} rule proposals · ${result.preserved_manual} manual overrides protected`,
    );
    return true;
  }
  async function applyRules() {
    if (!doc || !activeProfile) return;
    await run("Evaluating rules…", async () => { await stageRules(doc, activeProfile.id); });
  }
  async function selectProfile(id: string) {
    if (!libraryQuery.data) return;
    await run("Selecting profile…", async () => {
      const saved = await saveRuleLibrary({
        ...libraryQuery.data!,
        active_profile_id: id,
      });
      queryClient.setQueryData(["rule-library"], saved);
      onActivity("Profile selected — apply rules to refresh proposals");
    });
  }
  function releaseOverride(uid: string | null, property: Property) {
    dispatch({ type: "release", keys: [draftKey(uid, property)] });
    setPlan(undefined);
    setRuleReport(undefined);
    onActivity("Manual override released — apply rules to refresh");
  }
  async function browseNative() {
    await run("Choosing a file…", async () => {
      const inspected = await chooseFile();
      if (inspected) await showInspection(inspected);
      else onActivity("File selection cancelled");
    });
  }
  async function open(target = path.trim(), recent = false) {
    await run("Inspecting metadata…", async () =>
      showInspection(await (recent ? openRecent(target) : inspectFile(target))),
    );
  }
  async function preview() {
    if (!doc || !dirty || operation.current) return;
    dispatch({ type: "commit" });
    await run("Planning changes…", async () => {
      const next = await previewChanges(doc.id, changes);
      setPlan(next);
      onActivity(`${next.changes.length} properties staged for review`);
    });
  }
  async function write() {
    if (!plan) return;
    await run("Saving and verifying…", async () => {
      const result = await savePlan(plan.id);
      setReport(result);
      setPlan(undefined);
      dispatch({ type: "reset" });
      setDoc(result.inspection ?? undefined);
      onInspection?.(result.inspection ?? undefined);
      setRuleReport(undefined);
      setSelected(new Set());
      onActivity(
        result.success
          ? `Save verified · ${result.elapsed_ms} ms`
          : "Save needs attention — see verification report",
      );
    });
  }
  function revertTrack(uid: string) {
    if (!doc) return;
    apply(
      changes
        .filter((change) => change.track_uid === uid)
        .map((edit) =>
          makePatch(
            doc,
            uid,
            edit.property,
            originalValue(doc, uid, edit.property),
          ),
        ),
    );
  }
  function bulk() {
    if (!doc) return;
    const targets = doc.tracks.filter((track) => selected.has(track.uid));
    if (!targets.length) return;
    if (targets.some((track) => !track.editable.includes(bulkProperty))) {
      setError(
        "At least one selected track does not support this property. No bulk edits were staged.",
      );
      return;
    }
    const value =
      bulkProperty === "title"
        ? bulkValue || null
        : bulkProperty === "language"
          ? bulkValue
          : bulkValue === "true";
    apply(
      targets.map((track) => makePatch(doc, track.uid, bulkProperty, value)),
    );
    onActivity(`Bulk edit staged for ${targets.length} tracks`);
  }
  async function copySelected() {
    if (!doc) return;
    await run("Copying metadata…", async () => {
      const rows = doc.tracks
        .filter((track) => selected.has(track.uid))
        .map((track) =>
          (["title", "language"] as const).map((property) => {
            const patch = draft[draftKey(track.uid, property)];
            return String(
              patch
                ? (patch.value ?? "")
                : (originalValue(doc, track.uid, property) ?? ""),
            );
          }),
        );
      await navigator.clipboard.writeText(serializeTsv(rows));
      onActivity(`Copied titles and languages for ${rows.length} tracks`);
    });
  }
  function paste(
    event: ClipboardEvent<HTMLInputElement>,
    track: Track,
    property: "title" | "language",
  ) {
    const contents = event.clipboardData.getData("text/plain");
    if (!/[\t\r\n]/.test(contents) || !doc) return;
    event.preventDefault();
    try {
      const rows = parseTsv(contents);
      const start = doc.tracks.findIndex((item) => item.uid === track.uid);
      if (start + rows.length > doc.tracks.length)
        throw new Error(
          "Paste extends beyond the last track. No changes were staged.",
        );
      const patches: Patch[] = [];
      rows.forEach((row, index) =>
        row.forEach((value, column) => {
          const field: Property = column === 0 ? property : "language";
          const target = doc.tracks[start + index];
          if (
            (property === "language" && column > 0) ||
            !target.editable.includes(field)
          )
            throw new Error(
              "Paste includes an unavailable column or read-only track. No changes were staged.",
            );
          patches.push(
            makePatch(
              doc,
              target.uid,
              field,
              field === "title" ? value || null : value,
            ),
          );
        }),
      );
      apply(patches);
      onActivity(`Pasted ${patches.length} metadata values`);
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Could not paste metadata",
      );
    }
  }
  useEffect(() => {
    function shortcut(event: KeyboardEvent) {
      if (
        !active ||
        browser ||
        plan ||
        operation.current ||
        !(event.ctrlKey || event.metaKey)
      )
        return;
      const key = event.key.toLowerCase();
      if (
        (event.target as HTMLElement).closest(
          ".open-path, .selection-toolbar",
        ) &&
        key !== "s"
      )
        return;
      if (key === "s") {
        event.preventDefault();
        void preview();
      }
      if (key === "z") {
        event.preventDefault();
        if (event.shiftKey) redo();
        else undo();
      }
      if (key === "y") {
        event.preventDefault();
        redo();
      }
    }
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  });
  useEffect(() => {
    function leaving(event: BeforeUnloadEvent) {
      if (dirty || busy === "Saving and verifying…") event.preventDefault();
    }
    window.addEventListener("beforeunload", leaving);
    return () => window.removeEventListener("beforeunload", leaving);
  }, [dirty, busy]);
  const containerPatch = draft[draftKey(null, "title")];
  return (
    <div className="editor" aria-busy={!!busy}>
      {pending.data
        ?.filter((p) => !p.folder)
        .map((item) => (
          <div className="batch-source" key={item.id}>
            <span title={item.path}>Explorer: {prettyPath(item.path)}</span>
            <Button
              variant="outline"
              disabled={!!busy || dirty}
              title={
                dirty ? "Save or revert current edits first" : "Open selection"
              }
              onClick={() =>
                void run("Opening Explorer selection…", async () => {
                  await showInspection(await openPending(item.id));
                  await pending.refetch();
                })
              }
            >
              Open
            </Button>
            <Button
              variant="ghost"
              onClick={() =>
                void dismissPending(item.id).then(() => pending.refetch())
              }
            >
              Dismiss
            </Button>
          </div>
        ))}
      <form
        className="path-form open-path"
        onSubmit={(event) => {
          event.preventDefault();
          void open();
        }}
      >
        <Button
          type="button"
          disabled={!!busy || dirty}
          onClick={() => void browseNative()}
        >
          <FolderOpen />
          Open file
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={!!busy || dirty}
          onClick={() => setBrowser(true)}
        >
          <Folder />
          Browse / recent
        </Button>
        <input
          className="field"
          aria-label="Media file path"
          placeholder="Or paste a media path"
          value={path}
          onChange={(event) => setPath(event.target.value)}
          disabled={!!busy || dirty}
        />
        <Button variant="ghost" disabled={!!busy || dirty || !path.trim()}>
          Inspect path
        </Button>
      </form>
      <div className="profile-application-bar">
        <label>
          Profile
          <select
            className="field"
            aria-label="Active metadata profile"
            disabled={!!busy || !libraryQuery.data}
            value={activeProfile?.id ?? ""}
            onChange={(event) => void selectProfile(event.target.value)}
          >
            {libraryQuery.data?.profiles.map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name}
              </option>
            ))}
          </select>
        </label>
        <Button
          variant="outline"
          disabled={!!busy || !doc || !activeProfile || !doc.write_available}
          onClick={() => void applyRules()}
        >
          <Sparkles />
          Apply rules
        </Button>
        <span className="muted">
          {activeProfile?.auto_apply ? "Stages on open" : "Manual application"}{" "}
          · {Object.keys(history.proposals).length} proposals ·{" "}
          {Object.keys(history.manual).length} manual overrides
        </span>
        {Object.keys(history.manual).length > 0 && (
          <Button
            variant="ghost"
            disabled={!!busy}
            onClick={() => {
              dispatch({ type: "release", keys: Object.keys(history.manual) });
              setRuleReport(undefined);
              setPlan(undefined);
              onActivity("Manual overrides cleared ? apply rules to refresh");
            }}
          >
            Clear overrides
          </Button>
        )}
        {libraryQuery.isError && (
          <Button variant="ghost" onClick={() => void libraryQuery.refetch()}>
            Reload rules
          </Button>
        )}
      </div>
      {!doc && !hasLocations && (
        <p className="location-hint">
          Open file works anywhere. Choose a folder in Browse to explore media
          or reopen a recent file.
        </p>
      )}
      {error && (
        <div role="alert" className="connection-alert">
          <strong>Operation could not complete</strong>
          <p>{error}</p>
          <Button
            variant="ghost"
            size="icon"
            aria-label="Dismiss error"
            onClick={() => setError("")}
          >
            <X />
          </Button>
        </div>
      )}
      {!doc ? (
        <div className="editor-panel">
          <div className="empty-state">
            <div className="file-icon">
              <FileVideo size={26} strokeWidth={1.3} />
            </div>
            <h2>A place for every track.</h2>
            <p>Open an MKV or MKA to inspect and refine its metadata.</p>
            <div className="foundation-badge">
              <ShieldCheck size={13} />
              Changes stay staged until you review and save.
            </div>
          </div>
        </div>
      ) : (
        <>
          <div className="technical-status" role="status" aria-live="polite">
            {(doc.analysis?.status === "pending" || doc.sections_pending) && !enrichment.error ? (
              <>
                <RefreshCw size={14} className="batch-spinner" />
                {doc.analysis?.status === "pending" ? "Checking technical details… You can edit tracks now; technical names will follow." : "Loading container details… Verified track details are ready to edit."}
                <Button variant="ghost" disabled={!!busy} onClick={() => {
                  void run("Stopping technical analysis…", async () => {
                    const next = await cancelEnrichment(doc.id);
                    if (currentDoc.current?.id === next.id) { setDoc(next); onInspection?.(next); }
                  });
                }}>Stop checking</Button>
              </>
            ) : doc.analysis?.status === "failed" || doc.analysis?.status === "partial" || enrichment.error ? (
              <>
                Technical details are incomplete. {doc.analysis?.error}
                <Button variant="ghost" disabled={!!busy} title="Retry with full-file MediaInfo analysis; this can take longer." onClick={() => {
                  void run("Retrying technical analysis…", async () => {
                    const next = await getEnrichment(doc.id, true);
                    queryClient.setQueryData(["enrichment", doc.id], next);
                    if (currentDoc.current?.id === next.id) { setDoc(next); onInspection?.(next); }
                  });
                }}>Retry full analysis</Button>
              </>
            ) : (
              <>Technical details ready{doc.analysis?.cached ? " · reused verified facts" : ""}.</>
            )}
            {enrichment.error && <span>{enrichment.error.message}</span>}
          </div>
          <section className="editor-panel">
            <div className="media-heading">
              <div>
                <strong title={doc.path}>
                  {doc.path.split(/[\\/]/).pop()}
                </strong>
                <p title={doc.path}>{doc.path}</p>
              </div>
              <span className="media-summary">
                {doc.container} · {size(doc.size_bytes)} · {duration(doc)} ·{" "}
                {doc.tracks.length} tracks
              </span>
            </div>
            <div className="editor-toolbar">
              <label className="container-title">
                Container title
                <div className="cell-editor">
                  <input
                    data-source={
                      history.manual[draftKey(null, "title")]
                        ? "manual"
                        : history.proposals[draftKey(null, "title")]
                          ? "rules"
                          : "original"
                    }
                    className={`field ${containerPatch ? "modified" : ""}`}
                    aria-label="Container title"
                    value={String(
                      containerPatch
                        ? (containerPatch.value ?? "")
                        : (doc.title ?? ""),
                    )}
                    placeholder="Absent"
                    title={`Original: ${show(doc.title)}`}
                    disabled={!!busy || !doc.editable.includes("title")}
                    onBlur={() => dispatch({ type: "commit" })}
                    onChange={(event) =>
                      apply(
                        [
                          makePatch(
                            doc,
                            null,
                            "title",
                            event.target.value || null,
                          ),
                        ],
                        "container:title",
                      )
                    }
                  />
                  {containerPatch && (
                    <button
                      className="cell-revert"
                      title="Revert container title"
                      aria-label="Revert container title"
                      disabled={!!busy}
                      onClick={() =>
                        apply([
                          makePatch(doc, null, "title", doc.title ?? null),
                        ])
                      }
                    >
                      <RotateCcw size={12} />
                    </button>
                  )}
                  {history.manual[draftKey(null, "title")] && (
                    <button
                      className="cell-release"
                      title="Release manual override; apply rules to refresh"
                      aria-label="Release container-title override"
                      disabled={!!busy}
                      onClick={() => releaseOverride(null, "title")}
                    >
                      <LockKeyhole size={12} />
                    </button>
                  )}
                </div>
              </label>
              <div className="toolbar-actions">
                <Button
                  variant="ghost"
                  size="icon"
                  title="Undo · Ctrl+Z"
                  aria-label="Undo"
                  disabled={!!busy || !history.past.length}
                  onClick={undo}
                >
                  <Undo2 />
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  title="Redo · Ctrl+Shift+Z"
                  aria-label="Redo"
                  disabled={!!busy || !history.future.length}
                  onClick={redo}
                >
                  <Redo2 />
                </Button>
                <Button
                  variant="ghost"
                  disabled={!!busy || !dirty}
                  onClick={() => {
                    autoInspection.current = undefined;
                    dispatch({ type: "revert" });
                    setRuleReport(undefined);
                    setPlan(undefined);
                    setError("");
                    onActivity("Changes reverted");
                  }}
                >
                  <RotateCcw />
                  Revert
                </Button>
                <Button
                  variant="ghost"
                  size="icon"
                  title={
                    dirty
                      ? "Save or revert before refreshing"
                      : "Refresh metadata"
                  }
                  aria-label="Refresh metadata"
                  disabled={
                    !!busy || dirty || Object.keys(history.manual).length > 0
                  }
                  onClick={() => void open(doc.path)}
                >
                  <RefreshCw />
                </Button>
                <Button
                  disabled={!!busy || !dirty || !doc.write_available}
                  onClick={() => void preview()}
                >
                  <Save />
                  Review save
                </Button>
              </div>
            </div>
            <div
              className="content-tabs"
              role="tablist"
              aria-label="Container contents"
            >
              {[
                "Tracks",
                "Attachments",
                "Chapters",
                "Tags",
                "Container information",
              ].map((name) => {
                const count = sectionCount(doc, name);
                return (
                  <button
                    role="tab"
                    type="button"
                    aria-selected={tab === name}
                    key={name}
                    onClick={() => setTab(name)}
                  >
                    {name}
                    {count !== undefined && (
                      <span className="tab-count">{doc.sections_pending && (name === "Chapters" || name === "Tags") ? "…" : count}</span>
                    )}
                  </button>
                );
              })}
            </div>
            {tab === "Tracks" ? (
              <>
                <div className="selection-toolbar">
                  <select
                    className="field"
                    aria-label="Select tracks by type"
                    value=""
                    disabled={!!busy}
                    onChange={(event) => {
                      const kind = event.target.value;
                      setSelected(
                        new Set(
                          doc.tracks
                            .filter(
                              (track) => kind === "all" || track.kind === kind,
                            )
                            .map((track) => track.uid),
                        ),
                      );
                    }}
                  >
                    <option value="" disabled>
                      Select tracks…
                    </option>
                    <option value="all">All tracks</option>
                    <option value="video">Video</option>
                    <option value="audio">Audio</option>
                    <option value="subtitles">Subtitles</option>
                    <option value="none">None</option>
                  </select>
                  {selected.size > 0 && (
                    <>
                      <select
                        className="field"
                        aria-label="Bulk property"
                        value={bulkProperty}
                        disabled={!!busy}
                        onChange={(event) => {
                          const property = event.target.value as Property;
                          setBulkProperty(property);
                          setBulkValue(
                            ["title", "language"].includes(property)
                              ? ""
                              : "true",
                          );
                        }}
                      >
                        {(
                          [
                            "title",
                            "language",
                            "default",
                            "forced",
                            "enabled",
                            "hearing_impaired",
                            "commentary",
                            "original",
                            "visual_impaired",
                            "text_descriptions",
                          ] as Property[]
                        ).map((property) => (
                          <option key={property} value={property}>
                            {property.replaceAll("_", " ")}
                          </option>
                        ))}
                      </select>
                      {["title", "language"].includes(bulkProperty) ? (
                        <input
                          className="field bulk-value"
                          aria-label="Bulk value"
                          value={bulkValue}
                          list={
                            bulkProperty === "language"
                              ? "language-tags"
                              : undefined
                          }
                          placeholder={
                            bulkProperty === "title"
                              ? "Empty clears title"
                              : "BCP 47 language"
                          }
                          disabled={!!busy}
                          onChange={(event) => setBulkValue(event.target.value)}
                        />
                      ) : (
                        <select
                          className="field"
                          aria-label="Bulk flag value"
                          value={bulkValue}
                          disabled={!!busy}
                          onChange={(event) => setBulkValue(event.target.value)}
                        >
                          <option value="true">On</option>
                          <option value="false">Off</option>
                        </select>
                      )}
                      <Button
                        variant="outline"
                        disabled={
                          !!busy ||
                          (bulkProperty === "language" && !bulkValue.trim())
                        }
                        onClick={bulk}
                      >
                        Apply to {selected.size}
                      </Button>
                      <Button
                        variant="ghost"
                        title="Copy selected titles and languages as tab-separated columns"
                        disabled={!!busy}
                        onClick={() => void copySelected()}
                      >
                        Copy
                      </Button>
                    </>
                  )}
                </div>
                <TrackTable
                  key={doc.id}
                  doc={doc}
                  draft={draft}
                  manual={history.manual}
                  proposals={history.proposals}
                  onRelease={releaseOverride}
                  selected={selected}
                  busy={!!busy}
                  onSelection={setSelected}
                  onEdit={apply}
                  onCommit={() => dispatch({ type: "commit" })}
                  onPaste={paste}
                  onRevert={revertTrack}
                />
              </>
            ) : (
              <ContainerSection doc={doc} tab={tab} />
            )}
            <div className="editor-summary">
              <span>
                {modifiedTracks} modified tracks · {changes.length} properties ·{" "}
                {doc.write_available
                  ? "In-place editing available"
                  : "Read-only"}
              </span>
              <span>
                {busy || `Inspected in ${doc.elapsed_ms} ms`} · Mixed checkbox =
                absent
              </span>
            </div>
          </section>
          {(doc.warnings.length > 0 ||
            (!doc.sections_pending && (!doc.chapters.available || !doc.tags.available))) && (
            <details className="inspection-warnings">
              <summary>
                Inspection notes (
                {doc.warnings.length +
                  Number(!doc.sections_pending && !doc.chapters.available) +
                  Number(!doc.sections_pending && !doc.tags.available)}
                )
              </summary>
              <ul>
                {doc.warnings.map((note, index) => (
                  <li key={index}>{note}</li>
                ))}
                {!doc.sections_pending && !doc.chapters.available && (
                  <li>Chapters/editions: {doc.chapters.error}</li>
                )}
                {!doc.sections_pending && !doc.tags.available && <li>Tags: {doc.tags.error}</li>}
              </ul>
            </details>
          )}
        </>
      )}
      {plan && (
        <SavePreview
          plan={plan}
          doc={doc}
          busy={!!busy}
          onClose={() => setPlan(undefined)}
          onWrite={() => void write()}
        />
      )}
      {ruleReport && <RulePreviewPanel result={ruleReport} doc={doc} />}
      {report && (
        <section
          className={`save-report ${report.success ? "verified" : ""}`}
          role="status"
        >
          <h2>
            {report.success ? (
              <>
                <Check size={16} />
                Save verified
              </>
            ) : (
              "Save needs attention"
            )}
          </h2>
          <p>
            {report.status.replaceAll("_", " ")} · {report.elapsed_ms} ms{" "}
            <span className="muted">· {report.operation_id}</span>
          </p>
          {[...report.discrepancies, ...report.warnings].map((note, index) => (
            <p key={index}>{note}</p>
          ))}
          {report.restore_available && (
            <Button
              variant="outline"
              disabled={!!busy || dirty}
              onClick={() =>
                void run("Preparing restoration…", async () => {
                  setPlan(await restorePlan(report.operation_id));
                  onActivity("Restoration staged for review");
                })
              }
            >
              <Undo2 />
              Review metadata restoration
            </Button>
          )}
        </section>
      )}
      {browser && (
        <FileBrowser
          onClose={() => setBrowser(false)}
          onOpen={(target, recent) => {
            setBrowser(false);
            void open(target, recent);
          }}
        />
      )}
      <datalist id="language-tags">
        {languages.map((language) => (
          <option key={language.code} value={language.code}>
            {language.name}
          </option>
        ))}
      </datalist>
    </div>
  );
}

function SavePreview({
  plan,
  doc,
  busy,
  onClose,
  onWrite,
}: {
  plan: Plan;
  doc?: Inspection;
  busy: boolean;
  onClose: () => void;
  onWrite: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    dialog.current?.showModal();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="save-preview-dialog"
      aria-labelledby="save-preview-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
    >
      <header className="dialog-heading">
        <h2 id="save-preview-title">Review metadata changes</h2>
        <Button
          variant="ghost"
          size="icon"
          aria-label="Close preview"
          disabled={busy}
          onClick={onClose}
        >
          <X />
        </Button>
      </header>
      <div className="save-preview-body">
        <p className="muted">{plan.path}</p>
        {plan.changes.length ? (
          <table className="changes-table">
            <thead>
              <tr>
                <th>Target</th>
                <th>Property</th>
                <th>Original</th>
                <th>Proposed</th>
              </tr>
            </thead>
            <tbody>
              {plan.changes.map((change, index) => (
                <tr key={index}>
                  <td>
                    {change.track_uid
                      ? `Track ${doc?.tracks.find((track) => track.uid === change.track_uid)?.number ?? change.track_uid}`
                      : "Container"}
                  </td>
                  <td>{change.property.replaceAll("_", " ")}</td>
                  <td>{show(change.before)}</td>
                  <td>{show(change.after)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p>No effective changes. These values already match the file.</p>
        )}
        {plan.warnings.map((warning) => (
          <p key={warning} className="save-warning">
            {warning}
          </p>
        ))}
      </div>
      <footer className="preview-actions">
        <Button autoFocus variant="outline" disabled={busy} onClick={onClose}>
          Close preview
        </Button>
        <Button disabled={busy || !plan.changes.length} onClick={onWrite}>
          <Save />
          {busy ? "Saving and verifying…" : "Write metadata"}
        </Button>
      </footer>
    </dialog>
  );
}
