use super::{model::*, process};
use anyhow::Result;
use serde_json::Value;
use std::path::Path;

pub struct AdapterStatus {
    pub inspection_available: bool,
    pub writing_available: bool,
    pub diagnostics: Vec<String>,
}

/// Container-specific identification, capabilities, validation, planning, writing and verification.
pub trait ContainerAdapter: Send {
    fn status(&self) -> AdapterStatus;
    fn inspect(&self, path: &Path) -> Result<Inspection>;
    /// Track-only preview; saving must use a full inspection.
    fn inspect_batch(&self, path: &Path, _enrich: bool) -> Result<Inspection> {
        self.inspect(path)
    }
    fn plan_change(&self, document: &Inspection, edit: Edit) -> Result<Option<Change>>;
    fn validate_plan(&self, document: &Inspection, changes: &[Change]) -> Result<Vec<String>>;
    fn property_value(
        &self,
        document: &Inspection,
        uid: Option<&str>,
        property: Property,
    ) -> Result<Value>;
    fn write(&self, path: &Path, changes: &[Change]) -> Result<process::Output>;
    fn verify(&self, before: &Inspection, after: &Inspection, changes: &[Change]) -> Vec<String>;
}
