import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowDown,
  ArrowUp,
  Copy,
  Download,
  GripVertical,
  Plus,
  RefreshCw,
  Save,
  Sparkles,
  Trash2,
  Upload,
} from "lucide-react";
import {
  getRuleCatalog,
  getRuleLibrary,
  getStandardRules,
  previewRules,
  saveRuleLibrary,
  validateRuleLibrary,
  type Edit,
  type Inspection,
  type Profile,
  type Rule,
  type RuleLibrary,
  type RulePreview,
} from "../api/client";
import {
  exampleRules,
  newRule,
  policyLabels,
  reorderRules,
} from "../lib/rule-defaults";
import { RuleBuilder } from "./rule-builder";
import { RulePreviewPanel } from "./rule-preview";
import { Button } from "./ui/button";

export function RulesPage({
  doc,
  manualEdits,
  onActivity,
  onDirtyChange,
}: {
  doc?: Inspection;
  manualEdits: Edit[];
  onActivity: (message: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
}) {
  const query = useQuery({
    queryKey: ["rule-library"],
    queryFn: getRuleLibrary,
    staleTime: 30000,
  });
  const catalog = useQuery({
    queryKey: ["rule-catalog"],
    queryFn: getRuleCatalog,
    staleTime: Infinity,
  });
  const client = useQueryClient();
  const [library, setLibrary] = useState<RuleLibrary>();
  const [dirty, setDirty] = useState(false);
  const [tab, setTab] = useState<"rules" | "profiles">("rules");
  const [selected, setSelected] = useState("");
  const [profileId, setProfileId] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    onDirtyChange?.(dirty || busy);
  }, [dirty, busy, onDirtyChange]);
  const running = useRef(false);
  const [result, setResult] = useState<RulePreview>();
  const fileInput = useRef<HTMLInputElement>(null);
  const dragged = useRef("");
  const context = useRef("");
  context.current = JSON.stringify([doc?.id, doc?.analysis?.status, manualEdits, library?.revision]);
  useEffect(() => {
    if (query.data && !dirty) {
      setLibrary(structuredClone(query.data));
      setSelected((old) =>
        query.data.rules.some((rule) => rule.id === old)
          ? old
          : (query.data.rules[0]?.id ?? ""),
      );
      setProfileId((old) =>
        query.data.profiles.some((profile) => profile.id === old)
          ? old
          : query.data.active_profile_id,
      );
    }
  }, [query.data, dirty]);
  useEffect(() => {
    function leaving(event: BeforeUnloadEvent) {
      if (dirty) event.preventDefault();
    }
    window.addEventListener("beforeunload", leaving);
    return () => window.removeEventListener("beforeunload", leaving);
  }, [dirty]);
  useEffect(() => setResult(undefined), [doc?.id, doc?.analysis?.status, manualEdits]);
  async function run(task: () => Promise<void>) {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    setError("");
    try {
      await task();
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Rule operation failed",
      );
    } finally {
      running.current = false;
      setBusy(false);
    }
  }
  function update(change: (library: RuleLibrary) => RuleLibrary) {
    setLibrary((old) => (old ? change(old) : old));
    setDirty(true);
    setResult(undefined);
    setError("");
  }
  function addRule() {
    if (!library) return;
    const rule = newRule(
      Math.max(-1, ...library.rules.map((rule) => rule.priority)) + 1,
    );
    update((old) => ({
      ...old,
      rules: [...old.rules, rule],
      profiles: old.profiles.map((profile) =>
        profile.id === old.active_profile_id
          ? { ...profile, rule_ids: [...profile.rule_ids, rule.id] }
          : profile,
      ),
    }));
    setSelected(rule.id);
    setTab("rules");
  }
  function removeRule(id: string) {
    update((old) => ({
      ...old,
      rules: old.rules.filter((rule) => rule.id !== id),
      profiles: old.profiles.map((profile) => ({
        ...profile,
        rule_ids: profile.rule_ids.filter((rule) => rule !== id),
      })),
    }));
    if (selected === id) setSelected("");
  }
  function duplicate(rule: Rule) {
    const copy = {
      ...structuredClone(rule),
      id: crypto.randomUUID(),
      name: `${rule.name} copy`.slice(0, 128),
      enabled: false,
      priority:
        Math.max(-1, ...library!.rules.map((rule) => rule.priority)) + 1,
    };
    update((old) => ({
      ...old,
      rules: [...old.rules, copy],
      profiles: old.profiles.map((profile) =>
        profile.id === old.active_profile_id
          ? { ...profile, rule_ids: [...profile.rule_ids, copy.id] }
          : profile,
      ),
    }));
    setSelected(copy.id);
  }
  async function save() {
    if (!library) return;
    await run(async () => {
      const saved = await saveRuleLibrary(library);
      setLibrary(saved);
      setDirty(false);
      client.setQueryData(["rule-library"], saved);
      onActivity("Rules and profiles saved");
    });
  }
  async function test(rule?: Rule) {
    if (!library || !doc) return;
    const expected = context.current;
    await run(async () => {
      const result = await previewRules(
        doc.id,
        profileId || library.active_profile_id,
        manualEdits,
        rule,
        library,
      );
      if (context.current !== expected)
        throw new Error("The current file or overrides changed; test again.");
      setResult(result);
      onActivity("Rule preview complete — no file was written");
    });
  }
  async function importFile(file: File) {
    await run(async () => {
      if (file.size > 512 * 1024)
        throw new Error("Rule imports are limited to 512 KiB.");
      const imported = await validateRuleLibrary(JSON.parse(await file.text()));
      imported.revision = query.data?.revision ?? 0;
      setLibrary(imported);
      setDirty(true);
      setSelected(imported.rules[0]?.id ?? "");
      setProfileId(imported.active_profile_id);
      setResult(undefined);
      onActivity("Library imported for review; save to persist");
    });
  }
  async function exportFile() {
    if (!library) return;
    await run(async () => {
      const valid = await validateRuleLibrary(library);
      const url = URL.createObjectURL(
        new Blob([JSON.stringify(valid, null, 2) + "\n"], {
          type: "application/json",
        }),
      );
      const link = document.createElement("a");
      link.href = url;
      link.download = "tracksmith-rules.json";
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 1000);
      onActivity("Rules and profiles exported");
    });
  }
  function addProfile() {
    const profile: Profile = {
      id: crypto.randomUUID(),
      name: "New profile",
      description: "",
      rule_ids: [],
      preferred_languages: [],
      auto_apply: false,
      overwrite_policy: null,
    };
    update((old) => ({ ...old, profiles: [...old.profiles, profile] }));
    setProfileId(profile.id);
    setTab("profiles");
  }
  async function addStandardProfile() {
    if (!library) return;
    await run(async () => {
      const defaults = await getStandardRules();
      const ids = new Map(
        defaults.rules.map((rule) => [rule.id, crypto.randomUUID()]),
      );
      const additions = defaults.rules.map((rule) => ({
        ...rule,
        id: ids.get(rule.id)!,
      }));
      const profile: Profile = {
        ...defaults.profiles[0],
        id: crypto.randomUUID(),
        rule_ids: additions.map((rule) => rule.id),
      };
      const next = {
        ...library,
        rules: [...library.rules, ...additions],
        profiles: [...library.profiles, profile],
      };
      await validateRuleLibrary(next);
      update(() => next);
      setProfileId(profile.id);
      setSelected(additions[0].id);
      setTab("profiles");
      onActivity(
        "Standard profile added for review; save the library and select it as active to use it",
      );
    });
  }
  function duplicateProfile(source: Profile) {
    if (!library) return;
    const additions = library.rules
      .filter((rule) => source.rule_ids.includes(rule.id))
      .map((rule) => ({ ...structuredClone(rule), id: crypto.randomUUID() }));
    if (
      library.rules.length + additions.length > 128 ||
      library.profiles.length >= 32
    ) {
      setError(
        "Duplicating this profile exceeds the library limit; remove unused rules/profiles first.",
      );
      return;
    }
    const copy: Profile = {
      ...structuredClone(source),
      id: crypto.randomUUID(),
      name: `${source.name.slice(0, 120)} copy`,
      rule_ids: additions.map((rule) => rule.id),
    };
    update((old) => ({
      ...old,
      rules: [...old.rules, ...additions],
      profiles: [...old.profiles, copy],
    }));
    setProfileId(copy.id);
    onActivity(
      "Profile and its rules copied independently; save the library to keep them",
    );
  }
  function changeProfile(next: Profile) {
    update((old) => ({
      ...old,
      profiles: old.profiles.map((profile) =>
        profile.id === next.id ? next : profile,
      ),
    }));
  }
  const rules = library
    ? [...library.rules].sort(
        (a, b) =>
          a.priority - b.priority || (a.id === b.id ? 0 : a.id < b.id ? -1 : 1),
      )
    : [];
  const rule = library?.rules.find((rule) => rule.id === selected);
  const profile = library?.profiles.find((profile) => profile.id === profileId);
  if (!library || !catalog.data)
    return (
      <div className="details-panel">
        <p>
          {query.isError
            ? query.error.message
            : catalog.isError
              ? catalog.error.message
              : "Loading rules…"}
        </p>
        <Button
          variant="outline"
          onClick={() => {
            void query.refetch();
            void catalog.refetch();
          }}
        >
          Retry
        </Button>
      </div>
    );
  return (
    <div className="rules-workspace">
      <div className="rules-topbar">
        <div
          className="rule-view-tabs"
          role="tablist"
          aria-label="Rules and profiles"
        >
          <button
            role="tab"
            aria-selected={tab === "rules"}
            onClick={() => setTab("rules")}
          >
            Rules <span>{rules.length}</span>
          </button>
          <button
            role="tab"
            aria-selected={tab === "profiles"}
            onClick={() => setTab("profiles")}
          >
            Profiles <span>{library.profiles.length}</span>
          </button>
        </div>
        <div className="toolbar-actions">
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void addStandardProfile()}
            title="Add a fresh editable standard profile without replacing your existing rules"
          >
            Add standard profile
          </Button>
          <span className="muted">
            {dirty ? "Unsaved changes" : `Revision ${library.revision}`}
          </span>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => {
              setDirty(false);
              setResult(undefined);
              void query.refetch();
            }}
            title="Discard unsaved library changes and reload"
          >
            <RefreshCw />
            Reload
          </Button>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => fileInput.current?.click()}
          >
            <Upload />
            Import
          </Button>
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => void exportFile()}
          >
            <Download />
            Export
          </Button>
          <Button disabled={busy || !dirty} onClick={() => void save()}>
            <Save />
            Save library
          </Button>
        </div>
      </div>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
      {dirty && query.data && library.revision !== query.data.revision && (
        <p className="form-error">
          The saved library changed in another editor. Export your draft or
          reload before saving.
        </p>
      )}
      <div className="rule-testbar">
        <span>
          {doc
            ? `Current file: ${doc.path.split(/[\\/]/).pop()}`
            : "Open a file in Editor to resolve templates and test rules."}
        </span>
        <label>
          Test profile
          <select
            className="field"
            value={profileId}
            disabled={busy}
            onChange={(event) => {
              setProfileId(event.target.value);
              setResult(undefined);
            }}
          >
            {library.profiles.map((profile) => (
              <option key={profile.id} value={profile.id}>
                {profile.name}
              </option>
            ))}
          </select>
        </label>
        <Button
          variant="outline"
          disabled={busy || !doc}
          onClick={() => void test()}
        >
          <Sparkles />
          Test profile
        </Button>
        {tab === "rules" && (
          <Button
            variant="outline"
            disabled={busy || !doc || !rule}
            onClick={() => void test(rule)}
          >
            Test rule
          </Button>
        )}
      </div>
      <fieldset disabled={busy} className="rule-editing-region">
        {tab === "rules" ? (
          <div className="rule-columns">
            <aside className="rule-list">
              <div className="rule-list-heading">
                <Button
                  variant="outline"
                  disabled={rules.length >= 128}
                  onClick={addRule}
                >
                  <Plus />
                  Rule
                </Button>
                <Button
                  variant="ghost"
                  disabled={rules.length > 125}
                  onClick={() => {
                    const examples = exampleRules(
                      Math.max(-1, ...rules.map((rule) => rule.priority)) + 1,
                    );
                    update((old) => ({
                      ...old,
                      rules: [...old.rules, ...examples],
                      profiles: old.profiles.map((profile) =>
                        profile.id === old.active_profile_id
                          ? {
                              ...profile,
                              rule_ids: [
                                ...profile.rule_ids,
                                ...examples.map((rule) => rule.id),
                              ],
                            }
                          : profile,
                      ),
                    }));
                    setSelected(examples[0].id);
                  }}
                >
                  Examples
                </Button>
              </div>
              {!rules.length && (
                <p className="muted">
                  Add your own rule or editable, disabled examples. No naming
                  convention is applied by default.
                </p>
              )}
              {rules.map((item, index) => (
                <div
                  className={`rule-list-item ${selected === item.id ? "selected" : ""}`}
                  key={item.id}
                  draggable={!busy}
                  onDragStart={(event) => {
                    dragged.current = item.id;
                    event.dataTransfer.setData("text/plain", item.id);
                    event.dataTransfer.effectAllowed = "move";
                  }}
                  onDragEnd={() => {
                    dragged.current = "";
                  }}
                  onDragOver={(event) => event.preventDefault()}
                  onDrop={(event) => {
                    event.preventDefault();
                    update((old) =>
                      reorderRules(old, dragged.current, item.id),
                    );
                  }}
                >
                  <GripVertical size={13} />
                  <input
                    type="checkbox"
                    aria-label={`Enable ${item.name}`}
                    checked={item.enabled}
                    onChange={(event) =>
                      update((old) => ({
                        ...old,
                        rules: old.rules.map((rule) =>
                          rule.id === item.id
                            ? { ...rule, enabled: event.target.checked }
                            : rule,
                        ),
                      }))
                    }
                  />
                  <button
                    className="rule-name"
                    onClick={() => {
                      setSelected(item.id);
                      setResult(undefined);
                    }}
                    title={item.name}
                  >
                    <strong>{item.name || "Unnamed rule"}</strong>
                    <small>
                      Priority {item.priority} · {item.scope}
                    </small>
                  </button>
                  <div className="rule-order-buttons">
                    <button
                      disabled={index === 0}
                      title="Higher priority"
                      aria-label={`Move ${item.name} up`}
                      onClick={() =>
                        update((old) =>
                          reorderRules(old, item.id, rules[index - 1].id),
                        )
                      }
                    >
                      <ArrowUp size={12} />
                    </button>
                    <button
                      disabled={index === rules.length - 1}
                      title="Lower priority"
                      aria-label={`Move ${item.name} down`}
                      onClick={() =>
                        update((old) =>
                          reorderRules(old, item.id, rules[index + 1].id),
                        )
                      }
                    >
                      <ArrowDown size={12} />
                    </button>
                  </div>
                </div>
              ))}
            </aside>
            <section className="selected-rule">
              {rule ? (
                <>
                  <div className="builder-section-heading">
                    <h2>{rule.name || "Rule editor"}</h2>
                    <div className="toolbar-actions">
                      <Button
                        variant="ghost"
                        disabled={rules.length >= 128}
                        onClick={() => duplicate(rule)}
                      >
                        <Copy />
                        Duplicate
                      </Button>
                      <Button
                        variant="ghost"
                        onClick={() => removeRule(rule.id)}
                      >
                        <Trash2 />
                        Delete
                      </Button>
                    </div>
                  </div>
                  <RuleBuilder
                    rule={rule}
                    catalog={catalog.data}
                    onChange={(next) =>
                      update((old) => ({
                        ...old,
                        rules: old.rules.map((item) =>
                          item.id === next.id ? next : item,
                        ),
                      }))
                    }
                  />
                  <div className="rule-assignments">
                    <h3>Assigned profiles</h3>
                    {library.profiles.map((profile) => (
                      <label key={profile.id}>
                        <input
                          type="checkbox"
                          checked={profile.rule_ids.includes(rule.id)}
                          onChange={(event) =>
                            update((old) => ({
                              ...old,
                              profiles: old.profiles.map((item) =>
                                item.id === profile.id
                                  ? {
                                      ...item,
                                      rule_ids: event.target.checked
                                        ? [...item.rule_ids, rule.id]
                                        : item.rule_ids.filter(
                                            (id) => id !== rule.id,
                                          ),
                                    }
                                  : item,
                              ),
                            }))
                          }
                        />
                        {profile.name}
                      </label>
                    ))}
                  </div>
                </>
              ) : (
                <p className="muted">
                  Select a rule to edit its conditions, actions and overwrite
                  policy.
                </p>
              )}
            </section>
          </div>
        ) : (
          <div className="profile-columns">
            <aside className="profile-list">
              <Button
                variant="outline"
                disabled={library.profiles.length >= 32}
                onClick={addProfile}
              >
                <Plus />
                Profile
              </Button>
              {library.profiles.map((item) => (
                <button
                  className={profileId === item.id ? "selected" : ""}
                  key={item.id}
                  onClick={() => {
                    setProfileId(item.id);
                    setResult(undefined);
                  }}
                >
                  <strong>{item.name}</strong>
                  <small>
                    {item.rule_ids.length} assigned ·{" "}
                    {library.active_profile_id === item.id
                      ? "Active"
                      : item.auto_apply
                        ? "Auto on open"
                        : "Manual"}
                  </small>
                </button>
              ))}
            </aside>
            <section className="profile-editor">
              {profile && (
                <>
                  <div className="builder-section-heading">
                    <h2>{profile.name}</h2>
                    <div className="toolbar-actions">
                      <Button
                        variant="ghost"
                        onClick={() => duplicateProfile(profile)}
                        title="Copy this profile and its rules so edits remain independent"
                      >
                        <Copy />
                        Duplicate
                      </Button>
                      <Button
                        variant="outline"
                        disabled={library.active_profile_id === profile.id}
                        onClick={() =>
                          update((old) => ({
                            ...old,
                            active_profile_id: profile.id,
                          }))
                        }
                      >
                        {library.active_profile_id === profile.id
                          ? "Active profile"
                          : "Make active"}
                      </Button>
                      <Button
                        variant="ghost"
                        disabled={library.profiles.length === 1}
                        onClick={() => {
                          const remaining = library.profiles.filter(
                            (item) => item.id !== profile.id,
                          );
                          update((old) => ({
                            ...old,
                            profiles: remaining,
                            active_profile_id:
                              old.active_profile_id === profile.id
                                ? remaining[0].id
                                : old.active_profile_id,
                          }));
                          setProfileId(remaining[0].id);
                        }}
                      >
                        <Trash2 />
                        Delete
                      </Button>
                    </div>
                  </div>
                  <div className="rule-form-row">
                    <label>
                      Name
                      <input
                        className="field"
                        value={profile.name}
                        maxLength={128}
                        onChange={(event) =>
                          changeProfile({
                            ...profile,
                            name: event.target.value,
                          })
                        }
                      />
                    </label>
                    <label className="profile-auto">
                      <input
                        type="checkbox"
                        checked={profile.auto_apply}
                        onChange={(event) =>
                          changeProfile({
                            ...profile,
                            auto_apply: event.target.checked,
                          })
                        }
                      />
                      Stage rules automatically on open
                    </label>
                  </div>
                  <label className="rule-description">
                    Description
                    <input
                      className="field"
                      value={profile.description}
                      maxLength={2048}
                      onChange={(event) =>
                        changeProfile({
                          ...profile,
                          description: event.target.value,
                        })
                      }
                    />
                  </label>
                  <ProfileLanguages
                    key={profile.id}
                    profile={profile}
                    onChange={(values) =>
                      changeProfile({ ...profile, preferred_languages: values })
                    }
                  />
                  <p className="muted">
                    The first language supplies an explicit fallback action or
                    template variable. It is never reported as detected
                    language.
                  </p>
                  <label className="overwrite-label">
                    Profile overwrite behaviour
                    <select
                      className="field"
                      value={profile.overwrite_policy ?? ""}
                      onChange={(event) =>
                        changeProfile({
                          ...profile,
                          overwrite_policy: (event.target.value ||
                            null) as Profile["overwrite_policy"],
                        })
                      }
                    >
                      <option value="">Use each rule's policy</option>
                      {Object.entries(policyLabels)
                        .filter(([key]) => key !== "conditional")
                        .map(([key, label]) => (
                          <option key={key} value={key}>
                            {label}
                          </option>
                        ))}
                    </select>
                  </label>
                  <p className="muted">
                    Naming and track-flag policies are defined by assigned
                    rules. Conditional replacement conditions still apply when a
                    profile overrides overwrite behaviour.
                  </p>
                  <h3>Assigned rules</h3>
                  <div className="profile-rule-list">
                    {rules.map((rule) => (
                      <label key={rule.id}>
                        <input
                          type="checkbox"
                          checked={profile.rule_ids.includes(rule.id)}
                          onChange={(event) =>
                            changeProfile({
                              ...profile,
                              rule_ids: event.target.checked
                                ? [...profile.rule_ids, rule.id]
                                : profile.rule_ids.filter(
                                    (id) => id !== rule.id,
                                  ),
                            })
                          }
                        />
                        <span>
                          {rule.name}
                          <small>
                            {rule.enabled ? "Enabled" : "Disabled"} ·{" "}
                            {rule.scope} · priority {rule.priority}
                          </small>
                        </span>
                      </label>
                    ))}
                    {!rules.length && (
                      <p className="muted">
                        Create rules first, then assign them here.
                      </p>
                    )}
                  </div>
                </>
              )}
            </section>
          </div>
        )}
      </fieldset>
      {result && <RulePreviewPanel result={result} doc={doc} />}
      <p className="rule-evaluation-note">
        Lower priority numbers win simultaneous conflicts; equal priorities use
        rule IDs. Rules settle against staged metadata in up to eight passes.
        Cycles and missing values never trigger a file write. Tests and
        automatic application only stage proposals.
      </p>
      <input
        ref={fileInput}
        className="sr-only"
        disabled={busy}
        tabIndex={-1}
        type="file"
        accept="application/json,.json"
        onChange={(event) => {
          const file = event.target.files?.[0];
          event.target.value = "";
          if (file) void importFile(file);
        }}
      />
    </div>
  );
}

function ProfileLanguages({
  profile,
  onChange,
}: {
  profile: Profile;
  onChange: (values: string[]) => void;
}) {
  const joined = profile.preferred_languages.join(", ");
  const [text, setText] = useState(joined);
  const focused = useRef(false);
  useEffect(() => {
    if (!focused.current) setText(joined);
  }, [joined]);
  return (
    <label className="profile-languages">
      Preferred languages (comma separated BCP 47 tags)
      <input
        className="field"
        value={text}
        placeholder="No language fallback configured"
        onFocus={() => {
          focused.current = true;
        }}
        onBlur={() => {
          focused.current = false;
          setText(joined);
        }}
        onChange={(event) => {
          setText(event.target.value);
          onChange(
            event.target.value
              .split(",")
              .map((value) => value.trim())
              .filter(Boolean),
          );
        }}
      />
    </label>
  );
}
