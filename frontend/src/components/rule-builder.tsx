import { Plus, Trash2 } from "lucide-react";
import type {
  ConditionGroup,
  Rule,
  RuleAction,
  RuleCatalog,
} from "../api/client";
import { emptyGroup, policyLabels } from "../lib/rule-defaults";
import { Button } from "./ui/button";
import { languages } from "../lib/languages";

export function ConditionBuilder({
  group,
  catalog,
  onChange,
  depth = 0,
  onRemove,
}: {
  group: ConditionGroup;
  catalog: RuleCatalog;
  onChange: (group: ConditionGroup) => void;
  depth?: number;
  onRemove?: () => void;
}) {
  const update = (
    index: number,
    condition: ConditionGroup["conditions"][number],
  ) =>
    onChange({
      ...group,
      conditions: group.conditions.map((old, i) =>
        i === index ? condition : old,
      ),
    });
  return (
    <fieldset className="condition-group">
      <legend>
        <select
          className="field"
          aria-label="Condition group mode"
          value={group.mode}
          onChange={(event) =>
            onChange({
              ...group,
              mode: event.target.value as ConditionGroup["mode"],
            })
          }
        >
          <option value="all">All (AND)</option>
          <option value="any">Any (OR)</option>
        </select>
        {onRemove && (
          <Button
            size="icon"
            variant="ghost"
            aria-label="Remove condition group"
            onClick={onRemove}
          >
            <Trash2 />
          </Button>
        )}
      </legend>
      {group.conditions.map((condition, index) => {
        const definition = catalog.fields.find(
          (field) => field.field === condition.field,
        )!;
        const noValue = ["is_empty", "is_not_empty", "is_undefined"].includes(
          condition.operator,
        );
        return (
          <div className="condition-row" key={index}>
            <select
              className="field condition-field"
              aria-label="Matching property"
              value={condition.field}
              onChange={(event) => {
                const field = catalog.fields.find(
                  (field) => field.field === event.target.value,
                )!;
                update(index, {
                  ...condition,
                  field: field.field,
                  operator: field.operators[0],
                  value:
                    field.kind === "number"
                      ? 0
                      : field.kind === "boolean"
                        ? false
                        : "",
                });
              }}
            >
              {catalog.fields.map((field) => (
                <option key={field.field} value={field.field}>
                  {field.label}
                </option>
              ))}
            </select>
            <select
              className="field"
              aria-label="Matching operator"
              value={condition.operator}
              onChange={(event) => {
                const operator = event.target
                  .value as typeof condition.operator;
                update(index, {
                  ...condition,
                  operator,
                  value: ["is_empty", "is_not_empty", "is_undefined"].includes(
                    operator,
                  )
                    ? null
                    : definition.kind === "number"
                      ? 0
                      : definition.kind === "boolean"
                        ? false
                        : "",
                });
              }}
            >
              {definition.operators.map((operator) => (
                <option key={operator} value={operator}>
                  {operator.replaceAll("_", " ")}
                </option>
              ))}
            </select>
            {!noValue &&
              (definition.kind === "boolean" ? (
                <select
                  className="field condition-value"
                  aria-label="Condition value"
                  value={String(condition.value)}
                  onChange={(event) =>
                    update(index, {
                      ...condition,
                      value: event.target.value === "true",
                    })
                  }
                >
                  <option value="true">True</option>
                  <option value="false">False</option>
                </select>
              ) : (
                <input
                  className="field condition-value"
                  aria-label="Condition value"
                  type={definition.kind === "number" ? "number" : "text"}
                  value={String(condition.value ?? "")}
                  list={
                    condition.field === "language"
                      ? "rule-languages"
                      : condition.field === "track_type"
                        ? "rule-track-types"
                        : undefined
                  }
                  placeholder={
                    condition.field === "language"
                      ? "BCP 47 tag"
                      : condition.operator === "regex"
                        ? "Bounded regular expression"
                        : "Value"
                  }
                  onChange={(event) =>
                    update(index, {
                      ...condition,
                      value:
                        definition.kind === "number"
                          ? event.target.value === ""
                            ? null
                            : Number(event.target.value)
                          : event.target.value,
                    })
                  }
                />
              ))}
            {definition.kind === "text" && !noValue && (
              <label className="case-toggle" title="Case-sensitive matching">
                <input
                  type="checkbox"
                  checked={condition.case_sensitive}
                  onChange={(event) =>
                    update(index, {
                      ...condition,
                      case_sensitive: event.target.checked,
                    })
                  }
                />
                Aa
              </label>
            )}
            <Button
              variant="ghost"
              size="icon"
              aria-label="Remove condition"
              onClick={() =>
                onChange({
                  ...group,
                  conditions: group.conditions.filter((_, i) => i !== index),
                })
              }
            >
              <Trash2 />
            </Button>
          </div>
        );
      })}
      {group.groups.map((nested, index) => (
        <ConditionBuilder
          key={index}
          group={nested}
          catalog={catalog}
          depth={depth + 1}
          onChange={(next) =>
            onChange({
              ...group,
              groups: group.groups.map((old, i) => (i === index ? next : old)),
            })
          }
          onRemove={() =>
            onChange({
              ...group,
              groups: group.groups.filter((_, i) => i !== index),
            })
          }
        />
      ))}
      {!group.conditions.length && !group.groups.length && (
        <p className="muted group-empty">
          {group.mode === "all"
            ? "No conditions: every target in this scope matches."
            : "No conditions: this group does not match."}
        </p>
      )}
      <div className="builder-actions">
        <Button
          variant="ghost"
          onClick={() =>
            onChange({
              ...group,
              conditions: [
                ...group.conditions,
                {
                  field: "existing_title",
                  operator: "is_empty",
                  value: null,
                  case_sensitive: false,
                },
              ],
            })
          }
        >
          <Plus />
          Condition
        </Button>
        {depth < 4 && (
          <Button
            variant="ghost"
            onClick={() =>
              onChange({ ...group, groups: [...group.groups, emptyGroup()] })
            }
          >
            <Plus />
            Group
          </Button>
        )}
      </div>
    </fieldset>
  );
}
export function ActionBuilder({
  action,
  catalog,
  onChange,
  onRemove,
}: {
  action: RuleAction;
  catalog: RuleCatalog;
  onChange: (action: RuleAction) => void;
  onRemove: () => void;
}) {
  function property(value: RuleAction["property"]) {
    onChange({
      ...action,
      property: value,
      source: value === "title" ? "template" : "literal",
      value: value === "title" ? "{codec}" : value === "language" ? "" : true,
    });
  }
  const sources =
    action.property === "title"
      ? ["literal", "template", "clear"]
      : action.property === "language"
        ? ["literal", "preferred_language"]
        : ["literal"];
  return (
    <div className="action-card">
      <div className="action-row">
        <select
          className="field"
          aria-label="Action property"
          value={action.property}
          onChange={(event) =>
            property(event.target.value as RuleAction["property"])
          }
        >
          {[
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
          ].map((property) => (
            <option key={property} value={property}>
              {property.replaceAll("_", " ")}
            </option>
          ))}
        </select>
        <select
          className="field"
          aria-label="Action source"
          value={action.source}
          onChange={(event) => {
            const source = event.target.value as RuleAction["source"];
            onChange({
              ...action,
              source,
              value:
                source === "clear" || source === "preferred_language"
                  ? null
                  : source === "template"
                    ? "{codec}"
                    : action.property === "title" ||
                        action.property === "language"
                      ? ""
                      : true,
            });
          }}
        >
          {sources.map((source) => (
            <option key={source} value={source}>
              {source === "preferred_language"
                ? "Profile language fallback"
                : source === "clear"
                  ? "Clear title"
                  : source === "template"
                    ? "Title template"
                    : "Set value"}
            </option>
          ))}
        </select>
        {action.source === "literal" &&
        !["title", "language"].includes(action.property) ? (
          <select
            className="field action-value"
            aria-label="Flag value"
            value={String(action.value)}
            onChange={(event) =>
              onChange({ ...action, value: event.target.value === "true" })
            }
          >
            <option value="true">On</option>
            <option value="false">Off</option>
          </select>
        ) : (
          ["literal", "template"].includes(action.source) && (
            <input
              className="field action-value"
              aria-label="Action value"
              value={String(action.value ?? "")}
              placeholder={
                action.source === "template"
                  ? "Title template"
                  : action.property === "language"
                    ? "BCP 47 language"
                    : "Title"
              }
              list={
                action.property === "language" ? "rule-languages" : undefined
              }
              onChange={(event) =>
                onChange({ ...action, value: event.target.value })
              }
            />
          )
        )}
        <Button
          variant="ghost"
          size="icon"
          aria-label="Remove action"
          onClick={onRemove}
        >
          <Trash2 />
        </Button>
      </div>
      {action.source === "template" && (
        <div className="template-controls">
          <select
            className="field"
            aria-label="Insert template variable"
            value=""
            onChange={(event) =>
              onChange({
                ...action,
                value: `${String(action.value ?? "")}{${event.target.value}}`,
              })
            }
          >
            <option value="" disabled>
              Insert variable…
            </option>
            {catalog.variables.map((variable) => (
              <option key={variable.name} value={variable.name}>
                {variable.label} · {"{" + variable.name + "}"}
              </option>
            ))}
          </select>
          <span className="muted">
            Use {"{{"} and {"}}"} for literal braces.
          </span>
        </div>
      )}
      {["template", "preferred_language"].includes(action.source) && (
        <label className="missing-value-label">
          If a value is unavailable
          <select
            className="field"
            aria-label="Missing value policy"
            value={action.missing}
            onChange={(event) =>
              onChange({
                ...action,
                missing: event.target.value as RuleAction["missing"],
              })
            }
          >
            <option value="skip_action">Skip this action</option>
            <option value="skip_target">Skip this rule for the target</option>
          </select>
        </label>
      )}
      {action.source === "preferred_language" && (
        <p className="muted">
          Uses the profile's first preferred language. This is a user-configured
          fallback, not language detection.
        </p>
      )}
    </div>
  );
}
export function RuleBuilder({
  rule,
  catalog,
  onChange,
}: {
  rule: Rule;
  catalog: RuleCatalog;
  onChange: (rule: Rule) => void;
}) {
  const available: RuleAction["property"][] =
    rule.scope === "container"
      ? ["title"]
      : [
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
        ];
  const remaining = available.find(
    (property) => !rule.actions.some((action) => action.property === property),
  );
  function addAction() {
    if (!remaining) return;
    onChange({
      ...rule,
      actions: [
        ...rule.actions,
        {
          property: remaining,
          source:
            remaining === "title"
              ? "template"
              : remaining === "language"
                ? "preferred_language"
                : "literal",
          value:
            remaining === "title"
              ? "{codec}"
              : remaining === "language"
                ? null
                : true,
          missing: "skip_action",
        },
      ],
    });
  }
  return (
    <div className="rule-builder">
      <div className="rule-form-row">
        <label>
          Name
          <input
            className="field"
            value={rule.name}
            maxLength={128}
            onChange={(event) =>
              onChange({ ...rule, name: event.target.value })
            }
          />
        </label>
        <label>
          Scope
          <select
            className="field"
            value={rule.scope}
            onChange={(event) =>
              onChange({ ...rule, scope: event.target.value as Rule["scope"] })
            }
          >
            {[
              "container",
              "tracks",
              "video",
              "audio",
              "subtitles",
              "other",
            ].map((scope) => (
              <option key={scope} value={scope}>
                {scope}
              </option>
            ))}
          </select>
        </label>
        <label className="rule-enabled">
          <input
            type="checkbox"
            checked={rule.enabled}
            onChange={(event) =>
              onChange({ ...rule, enabled: event.target.checked })
            }
          />
          Enabled
        </label>
      </div>
      <label className="rule-description">
        Description
        <input
          className="field"
          value={rule.description}
          maxLength={2048}
          onChange={(event) =>
            onChange({ ...rule, description: event.target.value })
          }
        />
      </label>
      <h3>Matching conditions</h3>
      <ConditionBuilder
        group={rule.conditions}
        catalog={catalog}
        onChange={(conditions) => onChange({ ...rule, conditions })}
      />
      <div className="builder-section-heading">
        <h3>Actions</h3>
        <Button variant="ghost" disabled={!remaining} onClick={addAction}>
          <Plus />
          Action
        </Button>
      </div>
      {rule.actions.map((action, index) => (
        <ActionBuilder
          key={index}
          action={action}
          catalog={catalog}
          onChange={(next) =>
            onChange({
              ...rule,
              actions: rule.actions.map((old, i) => (i === index ? next : old)),
            })
          }
          onRemove={() =>
            onChange({
              ...rule,
              actions: rule.actions.filter((_, i) => i !== index),
            })
          }
        />
      ))}
      <label className="overwrite-label">
        Overwrite policy
        <select
          className="field"
          value={rule.overwrite}
          onChange={(event) => {
            const overwrite = event.target.value as Rule["overwrite"];
            onChange({
              ...rule,
              overwrite,
              replace_when:
                overwrite === "conditional"
                  ? (rule.replace_when ?? emptyGroup())
                  : rule.replace_when,
            });
          }}
        >
          {Object.entries(policyLabels).map(([value, label]) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      {rule.overwrite === "conditional" && (
        <>
          <p className="muted">Replacement must also match these conditions.</p>
          <ConditionBuilder
            group={rule.replace_when ?? emptyGroup()}
            catalog={catalog}
            onChange={(replace_when) => onChange({ ...rule, replace_when })}
          />
        </>
      )}
      <datalist id="rule-languages">
        {languages.map((language) => (
          <option key={language.code} value={language.code}>
            {language.name}
          </option>
        ))}
      </datalist>
      <datalist id="rule-track-types">
        {["video", "audio", "subtitles", "other"].map((kind) => (
          <option key={kind} value={kind} />
        ))}
      </datalist>
    </div>
  );
}
