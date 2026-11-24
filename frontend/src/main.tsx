import { StrictMode, useEffect, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  QueryClient,
  QueryClientProvider,
  useQuery,
} from "@tanstack/react-query";
import {
  Activity,
  AudioLines,
  BookOpen,
  ChevronRight,
  Info,
  Layers,
  ListFilter,
  Monitor,
  Moon,
  Settings,
  ShieldCheck,
  SlidersHorizontal,
  Sun,
} from "lucide-react";
import { Button } from "./components/ui/button";
import { Editor } from "./components/editor";
import { MediaLocations } from "./components/media-locations";
import { RulesPage } from "./components/rules-page";
import { BatchPage } from "./components/batch-page";
import { DesktopSettings } from "./components/desktop-settings";
import { ActivityPage } from "./components/activity-page";
import { UpdatesPanel } from "./components/updates-panel";
import { holdUpdateTabLock } from "./lib/update-lock";
import type { Edit, Inspection } from "./api/client";
import {
  bootstrapSession,
  getStatus,
  launchPage,
  getPending,
} from "./api/client";
import "./styles.css";

bootstrapSession();
const queryClient = new QueryClient();
type Theme = "system" | "light" | "dark";
function readTheme(): Theme {
  const value = localStorage.getItem("tracksmith.theme");
  return value === "dark" || value === "light" ? value : "system";
}
document.documentElement.dataset.theme = readTheme();

function App() {
  const [tabReady, setTabReady] = useState(false);
  useEffect(() => {
    void holdUpdateTabLock().then(
      () => setTabReady(true),
      () => setTabReady(true),
    );
  }, []);
  const [theme, setTheme] = useState<Theme>(readTheme);
  const [page, setPage] = useState(launchPage === "About" ? "About" : "Editor");
  const pending = useQuery({
    queryKey: ["desktop-pending"],
    queryFn: getPending,
    refetchInterval: 2000,
  });
  const [activity, setActivity] = useState("No file selected");
  const [editorKey, setEditorKey] = useState(0);
  const [inspection, setInspection] = useState<Inspection>();
  const [manualEdits, setManualEdits] = useState<Edit[]>([]);
  const [editorDirty, setEditorDirty] = useState(false);
  const [rulesDirty, setRulesDirty] = useState(false);
  const draftsClear =
    !editorDirty && !rulesDirty && (pending.data?.length ?? 0) === 0;
  const status = useQuery({
    queryKey: ["status"],
    queryFn: getStatus,
    retry: false,
    refetchInterval: 15000,
  });
  const data = status.data;
  function cycleTheme() {
    const next =
      theme === "system" ? "light" : theme === "light" ? "dark" : "system";
    setTheme(next);
    localStorage.setItem("tracksmith.theme", next);
    document.documentElement.dataset.theme = next;
  }
  const ThemeIcon =
    theme === "system" ? Monitor : theme === "dark" ? Moon : Sun;
  return (
    <div className="app-shell">
      <aside className="sidebar" aria-label="Main navigation" inert={!tabReady}>
        <div className="brand">
          <SlidersHorizontal size={23} strokeWidth={1.8} />
          <div>
            <strong>Tracksmith</strong>
            <span>Media metadata. Refined.</span>
          </div>
        </div>
        <div className="nav-label">WORKSPACE</div>
        <nav>
          {[
            { name: "Editor", icon: AudioLines, available: true },
            { name: "Batch editor", icon: Layers, available: true },
            { name: "Rules", icon: ListFilter, available: true },
            { name: "Settings", icon: Settings, available: true },
            { name: "Activity", icon: Activity, available: true },
            { name: "About", icon: Info, available: true },
          ].map(({ name, icon: Icon, available }) => (
            <button
              key={name}
              className={`nav-item ${page === name ? "active" : ""}`}
              disabled={!available}
              title={!available ? "Available in a later phase" : undefined}
              aria-current={page === name ? "page" : undefined}
              onClick={() => setPage(name)}
            >
              <Icon size={16} />
              {name}
              {!available && <span className="nav-soon">Later</span>}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <span className="version">v{data?.version ?? "1.0.0"} · Editor</span>
          <Button
            variant="ghost"
            size="icon"
            aria-label={`Theme: ${theme}. Switch theme`}
            title={`Theme: ${theme}`}
            onClick={cycleTheme}
          >
            <ThemeIcon />
          </Button>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            Workspace
            <ChevronRight size={13} />
            <span>{page}</span>
          </div>
          <span className="local-label">
            <ShieldCheck size={13} />
            Local session
          </span>
        </header>
        {!tabReady && (
          <p role="status">
            Waiting for another Tracksmith window to finish preparing its
            update…
          </p>
        )}
        <main inert={!tabReady}>
          <div className="page-heading">
            <div>
              <h1>{page === "Editor" ? "Media editor" : page}</h1>
              <p>
                {page === "Editor"
                  ? "Inspect and refine your media, one track at a time."
                  : page === "Settings"
                    ? "Your local application configuration."
                    : "A focused utility for the details that matter."}
              </p>
            </div>
          </div>
          {status.isError && (
            <div role="alert" className="connection-alert">
              <Info size={16} />
              <div>
                <strong>Connection needed</strong>
                <p>{status.error.message}</p>
              </div>
              <Button variant="outline" onClick={() => void status.refetch()}>
                Retry
              </Button>
            </div>
          )}
          <div hidden={page !== "Editor"}>
            <Editor
              key={editorKey}
              active={page === "Editor"}
              hasLocations={!!data?.media_root_count}
              onActivity={setActivity}
              onInspection={setInspection}
              onManualEdits={setManualEdits}
              onDirtyChange={setEditorDirty}
            />
          </div>
          {pending.data &&
            pending.data.length > 0 &&
            page !== "Editor" &&
            page !== "Batch editor" && (
              <p className="muted">
                Explorer selections are waiting in Editor / Batch editor.
              </p>
            )}
          {pending.data?.some((p) => p.folder) && page === "Editor" && (
            <Button variant="outline" onClick={() => setPage("Batch editor")}>
              Open selected folder in Batch editor
            </Button>
          )}
          <div hidden={page !== "Batch editor"}>
            <BatchPage active={page === "Batch editor"} />
          </div>
          {page === "Activity" && <ActivityPage />}
          <div hidden={page !== "Rules"}>
            <RulesPage
              doc={inspection}
              manualEdits={manualEdits}
              onActivity={setActivity}
              onDirtyChange={setRulesDirty}
            />
          </div>
          {page === "Settings" && (
            <section className="details-panel">
              <h2>Application</h2>
              <dl>
                <div>
                  <dt>Configuration</dt>
                  <dd>{data?.config_dir ?? "Connect to view configuration"}</dd>
                </div>
                <div>
                  <dt>Interface address</dt>
                  <dd>{window.location.origin}</dd>
                </div>
                <div>
                  <dt>Theme</dt>
                  <dd>
                    <Button variant="outline" onClick={cycleTheme}>
                      <ThemeIcon />
                      {theme}
                    </Button>
                  </dd>
                </div>
              </dl>
              <MediaLocations
                roots={data?.engine.roots ?? []}
                onSaved={() => {
                  void status.refetch();
                  setEditorKey((key) => key + 1);
                  setInspection(undefined);
                  setManualEdits([]);
                  setActivity("Media locations updated");
                }}
              />
              <DesktopSettings />
              <UpdatesPanel draftsClear={draftsClear} />
              <h2 className="tools-heading">Media utilities</h2>
              <p>
                {data?.engine.inspection_available
                  ? "MKVToolNix inspection is available."
                  : "Configure MKVToolNix to enable inspection."}
              </p>
              <p>
                {data?.engine.writing_available
                  ? "In-place Matroska editing is available."
                  : "Metadata writing is unavailable."}
              </p>
              {data?.engine.diagnostics.map((note) => (
                <p className="muted" key={note}>
                  {note}
                </p>
              ))}
              <p className="settings-note">
                Tool paths and the port can be changed in config.json. Restart
                Tracksmith after changing them.
              </p>
            </section>
          )}
          {page === "About" && (
            <section className="details-panel">
              <h2>
                Tracksmith{" "}
                <span className="muted">{data?.version ?? "1.0.0"}</span>
              </h2>
              <p>Media metadata. Refined.</p>
              <UpdatesPanel draftsClear={draftsClear} />
              <p>
                A local media metadata editor for Windows. Built with Rust and
                React.
              </p>
              <div className="about-note">
                <BookOpen size={17} />
                <p>
                  MKV/MKA inspection and editing with reviewed saves,
                  verification and metadata restoration. Browse folders, edit
                  multiple tracks and undo staged changes. User-defined rules,
                  templates and profiles stage proposals while preserving manual
                  overrides, reviewed batches and Windows tray integration.
                </p>
              </div>
            </section>
          )}
        </main>
        <footer className="statusbar" role="status">
          <span>
            <i
              className={status.isSuccess ? "status-dot ready" : "status-dot"}
            />
            {status.isSuccess
              ? "Local service connected"
              : status.isPending
                ? "Connecting…"
                : "Service disconnected"}
          </span>
          <span>{activity}</span>
        </footer>
      </div>
    </div>
  );
}
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </StrictMode>,
);
