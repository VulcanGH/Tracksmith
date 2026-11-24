import type {
  Rule,
  RuleLibrary,
  ConditionGroup,
  RuleAction,
} from "../api/client";

export const emptyGroup = (): ConditionGroup => ({
  mode: "all",
  conditions: [],
  groups: [],
});
export const newRule = (priority: number): Rule => ({
  id: crypto.randomUUID(),
  name: "New rule",
  description: "",
  enabled: false,
  priority: Math.min(priority, 4294967295),
  scope: "audio",
  conditions: emptyGroup(),
  actions: [
    {
      property: "title",
      source: "template",
      value: "{codec}",
      missing: "skip_action",
    },
  ],
  overwrite: "when_empty",
  replace_when: null,
});
export function exampleRules(priority: number): Rule[] {
  const make = (
    name: string,
    scope: Rule["scope"],
    conditions: ConditionGroup["conditions"],
    action: RuleAction,
  ): Rule => ({
    ...newRule(priority++),
    name,
    scope,
    conditions: { mode: "all", conditions, groups: [] },
    actions: [action],
  });
  return [
    make(
      "Example: reported TrueHD Atmos title",
      "audio",
      [
        {
          field: "codec",
          operator: "contains",
          value: "TrueHD",
          case_sensitive: false,
        },
        {
          field: "extended_format",
          operator: "contains",
          value: "Atmos",
          case_sensitive: false,
        },
      ],
      {
        property: "title",
        source: "template",
        value: "Dolby Atmos {channel_count}ch",
        missing: "skip_action",
      },
    ),
    {
      ...make(
        "Example: undefined audio language",
        "audio",
        [
          {
            field: "language",
            operator: "is_undefined",
            value: null,
            case_sensitive: false,
          },
        ],
        {
          property: "language",
          source: "preferred_language",
          value: null,
          missing: "skip_action",
        },
      ),
      overwrite: "when_undefined",
    },
    make(
      "Example: English hearing-impaired subtitles",
      "subtitles",
      [
        {
          field: "language",
          operator: "equals",
          value: "en",
          case_sensitive: false,
        },
        {
          field: "hearing_impaired",
          operator: "equals",
          value: true,
          case_sensitive: false,
        },
      ],
      {
        property: "title",
        source: "template",
        value: "{language_name} (SDH)",
        missing: "skip_action",
      },
    ),
  ];
}
export const policyLabels: Record<string, string> = {
  always: "Always overwrite",
  when_empty: "Only when empty",
  when_undefined: "Only when undefined",
  preserve_existing: "Preserve existing values",
  conditional: "Conditional replacement",
};
export function reorderRules(
  library: RuleLibrary,
  from: string,
  to: string,
): RuleLibrary {
  const rules = [...library.rules].sort(
    (a, b) =>
      a.priority - b.priority || (a.id === b.id ? 0 : a.id < b.id ? -1 : 1),
  );
  const source = rules.findIndex((rule) => rule.id === from),
    target = rules.findIndex((rule) => rule.id === to);
  if (source < 0 || target < 0) return library;
  const [item] = rules.splice(source, 1);
  rules.splice(target, 0, item);
  return {
    ...library,
    rules: rules.map((rule, index) => ({ ...rule, priority: index })),
  };
}
