//! Metadata engine: no HTTP or frontend dependencies. External utilities parse containers.
mod adapter;
mod browser;
mod engine;
mod filesystem;
mod matroska;
pub mod mediainfo;
pub mod model;
mod process;
pub mod rules;
pub use engine::{BatchTicket, Engine};
pub fn validate_selection(path: &std::path::Path) -> anyhow::Result<std::path::PathBuf> {
    let mut boundary = filesystem::Boundary::new(&[])?;
    if path.is_dir() {
        boundary.grant_selected_folder(path)
    } else {
        boundary.grant_selected_file(path)
    }
}
