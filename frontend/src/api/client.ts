import { z } from "zod";
import type { components } from "./schema";

export type Status = components["schemas"]["Status"];
export const launchPage = new URLSearchParams(
  window.location.hash.slice(1),
).get("page");
export const launchUpdateCheck =
  new URLSearchParams(window.location.hash.slice(1)).get("check") === "1";
// The generated type checks this runtime validator against the backend schema.
const statusSchema: z.ZodType<Status> = z.object({
  name: z.string(),
  version: z.string(),
  phase: z.string(),
  config_dir: z.string(),
  media_root_count: z.number().int().nonnegative(),
  editing_available: z.boolean(),
  engine: z.object({
    roots: z.array(z.string()),
    inspection_available: z.boolean(),
    writing_available: z.boolean(),
    diagnostics: z.array(z.string()),
  }),
});

export function bootstrapSession() {
  const params = new URLSearchParams(window.location.hash.slice(1));
  const token = params.get("token");
  if (token) {
    sessionStorage.setItem("tracksmith.session", token);
    history.replaceState(null, "", window.location.pathname);
  }
}

export async function getStatus(): Promise<Status> {
  const token = sessionStorage.getItem("tracksmith.session");
  if (!token)
    throw new Error(
      "Open the session link printed by Tracksmith to connect this tab.",
    );
  const response = await fetch("/api/status", {
    headers: { Authorization: `Bearer ${token}` },
    signal: AbortSignal.timeout(5000),
  });
  if (response.status === 401)
    throw new Error(
      "Session expired. Open the new link printed by Tracksmith.",
    );
  if (!response.ok)
    throw new Error(`Local service returned ${response.status}.`);
  return statusSchema.parse(await response.json());
}

type Models = components["schemas"];
export type Inspection = Models["Inspection"];
export type Track = Models["Track"];
export type Edit = Models["Edit"];
export type Plan = Models["Plan"];
export type SaveReport = Models["SaveReport"];
export type Property = Models["Property"];
async function post<T>(path: string, body: unknown): Promise<T> {
  const token = sessionStorage.getItem("tracksmith.session");
  if (!token) throw new Error("Open the Tracksmith session link to connect.");
  // Writes deliberately have no client timeout: an active in-place save must finish.
  const response = await fetch(`/api/${path}`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify(body),
  });
  if (!response.ok) {
    const text = await response.text();
    let message = text;
    try {
      message = (JSON.parse(text) as { message?: string }).message ?? text;
    } catch {
      /* Framework validation can return plain text. */
    }
    throw new Error(message || `Local service returned ${response.status}`);
  }
  return response.json() as Promise<T>;
}
export const inspectFile = (path: string) =>
  post<Inspection>("inspect", { path });
async function nativePicker<T>(endpoint: string): Promise<T | null> {
  const marker = Array.from(
    crypto.getRandomValues(new Uint8Array(16)),
    (byte) => byte.toString(16).padStart(2, "0"),
  ).join("");
  const previousTitle = document.title;
  const pickerTitle = `Tracksmith ${marker}`;
  document.title = pickerTitle;
  const restoreTitle = () => {
    if (document.title === pickerTitle) document.title = previousTitle;
  };
  const timer = window.setTimeout(restoreTitle, 2200);
  try {
    return await post<T | null>(endpoint, { marker });
  } finally {
    window.clearTimeout(timer);
    restoreTitle();
  }
}
export const chooseFile = () => nativePicker<Inspection>("pick-file");
export type DirectoryListing = Models["DirectoryListing"];
export const chooseFolder = () => nativePicker<DirectoryListing>("pick-folder");
export const browseDirectory = (
  path: string | null = null,
  search = "",
  offset = 0,
) => post<DirectoryListing>("browser", { path, search, offset });
export const openRecent = (path: string) =>
  post<Inspection>("open-recent", { path });
export const openLocation = (path: string) =>
  post<DirectoryListing>("open-location", { path });
export const setFavourite = (path: string, favourite: boolean) =>
  post<Models["EngineStatus"]>("favourite", { path, favourite });
export const previewChanges = (inspection_id: string, edits: Edit[]) =>
  post<Plan>("plans", { inspection_id, edits });
export const savePlan = (plan_id: string) =>
  post<SaveReport>("save", { plan_id });
export const restorePlan = (operation_id: string) =>
  post<Plan>("restore-plan", { operation_id });
export const setLocations = (roots: string[]) =>
  post<Models["EngineStatus"]>("locations", { roots });

export type RuleLibrary = Models["RuleLibrary"];
export type Rule = Models["Rule"];
export type Profile = Models["Profile"];
export type RulePreview = Models["RulePreview"];
export type RuleCatalog = Models["RuleCatalog"];
export type ConditionGroup = Models["ConditionGroup"];
export type Condition = Models["Condition"];
export type RuleAction = Models["Action"];
export type OverwritePolicy = Models["OverwritePolicy"];
async function get<T>(path: string): Promise<T> {
  const token = sessionStorage.getItem("tracksmith.session");
  if (!token) throw new Error("Open the Tracksmith session link to connect.");
  const response = await fetch(`/api/${path}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!response.ok) {
    const text = await response.text();
    let message = text;
    try {
      message = (JSON.parse(text) as { message?: string }).message ?? text;
    } catch {
      /* Plain framework errors remain readable. */
    }
    throw new Error(message);
  }
  return response.json() as Promise<T>;
}
export const getRuleLibrary = () => get<RuleLibrary>("rules");
export type UpdateStatus = Models["UpdateStatus"];
export const getUpdates = () => get<UpdateStatus>("updates");
export const checkUpdates = () => post<UpdateStatus>("updates/check", {});
export const installUpdate = (version: string) =>
  post<UpdateStatus>("updates/install", { version, drafts_clear: true });
export const getStandardRules = () => get<RuleLibrary>("rules/standard");
export type BatchView = Models["BatchView"];
export type BatchRow = Models["BatchRow"];
export type Integration = Models["Integration"];
export const getBatch = () => get<BatchView>("batch");
export const getBatchHistory = () => get<BatchView[]>("batch/history");
export const previewBatch = (body: Models["BatchRequest"]) =>
  post<BatchView>("batch/preview", body);
export const saveBatch = (job_id: string, selected: string[]) =>
  post<BatchView>("batch/save", { job_id, selected });
export const cancelBatch = () => post<BatchView>("batch/cancel", {});
export const clearBatch = () => post<BatchView>("batch/clear", {});
export const chooseBatchFile = () => nativePicker<string>("batch/pick-file");
export const getPending = () => get<Models["LaunchItem"][]>("desktop/pending");
export const openPending = (id: string) =>
  post<Inspection>("desktop/open", { id });
export const folderPending = (id: string) =>
  post<DirectoryListing>("desktop/folder", { id });
export const dismissPending = (id: string) =>
  post<boolean>("desktop/dismiss", { id });
export const getIntegration = () => get<Integration>("desktop/settings");
export const saveIntegration = (settings: Integration) =>
  post<Integration>("desktop/settings", settings);
export const stopApplication = () => post<boolean>("desktop/stop", {});
export const getRuleCatalog = () => get<RuleCatalog>("rules/catalog");
export const saveRuleLibrary = (library: RuleLibrary) =>
  post<RuleLibrary>("rules", library);
export const validateRuleLibrary = (library: unknown) =>
  post<RuleLibrary>("rules/validate", library);
export const previewRules = (
  inspection_id: string,
  profile_id: string,
  manual_edits: Edit[],
  test_rule?: Rule,
  test_library?: RuleLibrary,
) =>
  post<RulePreview>("rules/preview", {
    inspection_id,
    profile_id,
    manual_edits,
    test_rule: test_rule ?? null,
    test_library: test_library ?? null,
  });
