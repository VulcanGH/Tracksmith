use crate::media::model::{Edit, Property};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Container,
    HasVideo,
    Filename,
    FilePath,
    TrackType,
    Codec,
    NamingCodec,
    CodecProfile,
    ExtendedFormat,
    ChannelCount,
    ChannelLayout,
    Bitrate,
    SampleRate,
    BitDepth,
    Width,
    Height,
    Resolution,
    HdrFormat,
    DolbyVisionProfile,
    ExistingTitle,
    Language,
    LanguageName,
    TrackPosition,
    Default,
    Forced,
    Enabled,
    HearingImpaired,
    Commentary,
    Original,
    VisualImpaired,
    TextDescriptions,
    Lossless,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Operator {
    Equals,
    DoesNotEqual,
    Contains,
    DoesNotContain,
    StartsWith,
    EndsWith,
    Regex,
    GreaterThan,
    LessThan,
    IsEmpty,
    IsNotEmpty,
    IsUndefined,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    All,
    Any,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: Field,
    pub operator: Operator,
    pub value: Value,
    #[serde(default)]
    pub case_sensitive: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ConditionGroup {
    pub mode: MatchMode,
    pub conditions: Vec<Condition>,
    #[schema(no_recursion)]
    pub groups: Vec<ConditionGroup>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Container,
    Tracks,
    Video,
    Audio,
    Subtitles,
    Other,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverwritePolicy {
    Always,
    WhenEmpty,
    WhenUndefined,
    PreserveExisting,
    Conditional,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ValueSource {
    Literal,
    Template,
    PreferredLanguage,
    Clear,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MissingValue {
    SkipAction,
    SkipTarget,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub property: Property,
    pub source: ValueSource,
    pub value: Value,
    pub missing: MissingValue,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    /// Lower numbers run first. Equal priorities are resolved by rule ID.
    pub priority: u32,
    pub scope: Scope,
    pub conditions: ConditionGroup,
    pub actions: Vec<Action>,
    pub overwrite: OverwritePolicy,
    pub replace_when: Option<ConditionGroup>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub rule_ids: Vec<String>,
    pub preferred_languages: Vec<String>,
    pub auto_apply: bool,
    pub overwrite_policy: Option<OverwritePolicy>,
}
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RuleLibrary {
    pub version: u32,
    pub revision: u32,
    pub active_profile_id: String,
    pub rules: Vec<Rule>,
    pub profiles: Vec<Profile>,
}
impl Default for RuleLibrary {
    fn default() -> Self {
        super::standard::library()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EditKey {
    pub track_uid: Option<String>,
    pub property: Property,
}
#[derive(Clone, Serialize, ToSchema)]
pub struct RuleProposal {
    pub edit: Edit,
    pub before: Value,
    pub rule_id: String,
    pub rule_name: String,
}
#[derive(Serialize, ToSchema)]
pub struct RuleConflict {
    pub pass: usize,
    pub winner_id: String,
    pub ignored_id: String,
    pub target: EditKey,
    pub winner: String,
    pub ignored: String,
    pub winning_value: Value,
    pub ignored_value: Value,
}
#[derive(Serialize, ToSchema)]
pub struct RulePreview {
    pub inspection_id: String,
    pub profile_id: String,
    pub proposals: Vec<RuleProposal>,
    pub conflicts: Vec<RuleConflict>,
    pub notes: Vec<String>,
    pub passes: usize,
    pub preserved_manual: usize,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RulePreviewRequest {
    pub inspection_id: String,
    pub profile_id: String,
    pub manual_edits: Vec<Edit>,
    pub test_rule: Option<Rule>,
    pub test_library: Option<RuleLibrary>,
}
#[derive(Serialize, ToSchema)]
pub struct FieldDefinition {
    pub field: Field,
    pub label: String,
    pub kind: String,
    pub operators: Vec<Operator>,
}
#[derive(Serialize, ToSchema)]
pub struct TemplateVariable {
    pub name: String,
    pub label: String,
}
#[derive(Serialize, ToSchema)]
pub struct RuleCatalog {
    pub fields: Vec<FieldDefinition>,
    pub variables: Vec<TemplateVariable>,
}
