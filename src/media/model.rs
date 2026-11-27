use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Property {
    Title,
    Language,
    Default,
    Forced,
    Enabled,
    HearingImpaired,
    Commentary,
    Original,
    VisualImpaired,
    TextDescriptions,
}
impl Property {
    pub const ALL: [Self; 10] = [
        Self::Title,
        Self::Language,
        Self::Default,
        Self::Forced,
        Self::Enabled,
        Self::HearingImpaired,
        Self::Commentary,
        Self::Original,
        Self::VisualImpaired,
        Self::TextDescriptions,
    ];
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct Track {
    pub id: u32,
    /// Decimal string: a Matroska UID may exceed JavaScript's integer precision.
    pub uid: String,
    pub number: String,
    pub kind: String,
    pub codec: String,
    pub title: Option<String>,
    pub language: Option<String>,
    pub language_ietf: Option<String>,
    pub editable: Vec<Property>,
    pub flags: std::collections::BTreeMap<String, Option<bool>>,
    pub technical: Value,
    pub mediainfo: Option<Value>,
    pub mapping: String,
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct XmlElement {
    pub name: String,
    pub text: Option<String>,
    pub attributes: std::collections::BTreeMap<String, String>,
    #[schema(no_recursion)]
    pub children: Vec<XmlElement>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct MetadataSection {
    pub available: bool,
    pub xml: Option<String>,
    pub tree: Option<XmlElement>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct TechnicalState {
    /// pending, ready, partial, failed, or not_requested.
    pub status: String,
    /// full, audio, audio_sample, or none.
    pub scope: String,
    pub cached: bool,
    pub error: Option<String>,
}
impl Default for TechnicalState {
    fn default() -> Self {
        Self {
            status: "ready".into(),
            scope: "full".into(),
            cached: false,
            error: None,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct Inspection {
    pub id: String,
    pub path: String,
    pub size_bytes: String,
    pub container: String,
    pub title: Option<String>,
    pub editable: Vec<Property>,
    pub tracks: Vec<Track>,
    pub attachments: Value,
    pub chapters: MetadataSection,
    pub tags: MetadataSection,
    #[serde(default)]
    pub sections_pending: bool,
    pub container_info: Value,
    /// Complete identification output, retaining unknown components.
    pub identification: Value,
    pub mediainfo: Option<Value>,
    #[serde(default)]
    pub analysis: TechnicalState,
    pub warnings: Vec<String>,
    pub write_available: bool,
    pub elapsed_ms: u64,
}

#[derive(Clone, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    pub track_uid: Option<String>,
    pub property: Property,
    /// Text, boolean, or null to remove a supported optional value.
    pub value: Value,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct Change {
    pub track_uid: Option<String>,
    pub property: Property,
    pub before: Value,
    pub after: Value,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct Plan {
    pub id: String,
    pub inspection_id: String,
    pub path: String,
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize, ToSchema)]
pub struct SaveReport {
    pub operation_id: String,
    pub success: bool,
    pub status: String,
    pub discrepancies: Vec<String>,
    pub warnings: Vec<String>,
    pub inspection: Option<Inspection>,
    pub elapsed_ms: u64,
    pub restore_available: bool,
}
#[derive(Clone, Serialize, ToSchema)]
pub struct EngineStatus {
    pub roots: Vec<String>,
    pub inspection_available: bool,
    pub writing_available: bool,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Serialize, ToSchema)]
pub struct BrowserLocation {
    pub path: String,
    pub kind: String,
    pub authorized: bool,
    pub favourite: bool,
}
#[derive(Serialize, ToSchema)]
pub struct BrowserEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size_bytes: Option<String>,
}
#[derive(Serialize, ToSchema)]
pub struct DirectoryListing {
    pub offset: usize,
    pub path: Option<String>,
    pub parent: Option<String>,
    pub locations: Vec<BrowserLocation>,
    pub recent_files: Vec<BrowserLocation>,
    pub entries: Vec<BrowserEntry>,
    pub next_offset: Option<usize>,
    pub total_entries: usize,
    pub truncated: bool,
    pub notes: Vec<String>,
}
