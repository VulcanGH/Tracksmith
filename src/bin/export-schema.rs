use std::{fs, path::PathBuf};
use utoipa::OpenApi;
fn main() -> anyhow::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("frontend/openapi.json");
    fs::write(path, tracksmith::api::ApiDoc::openapi().to_pretty_json()?)?;
    Ok(())
}
