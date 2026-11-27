use crate::media::rules::{self, model::*};
use crate::media::{Engine, model::*};
use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};
use utoipa::{OpenApi, ToSchema};

#[derive(Clone)]
pub struct AppState {
    pub config_dir: String,
    pub engine: Arc<Mutex<Engine>>,
    pub engine_status: Arc<RwLock<EngineStatus>>,
    pub batch: Arc<crate::batch::Batch>,
    pub desktop: Arc<crate::lifecycle::Desktop>,
    pub updates: Arc<crate::update::Updater>,
    pub analysis_slots: Arc<tokio::sync::Semaphore>,
}
#[derive(Serialize, ToSchema)]
pub struct Status {
    pub name: String,
    pub version: String,
    pub phase: String,
    pub config_dir: String,
    pub media_root_count: usize,
    pub editing_available: bool,
    pub engine: EngineStatus,
}
#[derive(Serialize, ToSchema)]
pub struct ApiError {
    pub message: String,
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (StatusCode::UNPROCESSABLE_ENTITY, Json(self)).into_response()
    }
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct OpenRequest {
    pub path: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PickerRequest {
    pub marker: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct BrowserRequest {
    pub path: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub search: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FavouriteRequest {
    pub path: String,
    pub favourite: bool,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub inspection_id: String,
    pub edits: Vec<Edit>,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct SaveRequest {
    pub plan_id: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct RestoreRequest {
    pub operation_id: String,
}
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct LocationsRequest {
    pub roots: Vec<String>,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct IdRequest {
    pub id: String,
}

#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct EnrichmentRequest {
    pub id: String,
    #[serde(default)]
    pub retry: bool,
    #[serde(default)]
    pub cancel: bool,
}

async fn finish_open(
    state: Arc<AppState>,
    task: crate::media::InspectionTask,
) -> Result<Json<Inspection>, ApiError> {
    let (task, result) = tokio::task::spawn_blocking(move || {
        let result = task.run();
        (task, result)
    })
    .await
    .map_err(|_| failure(anyhow::anyhow!("Inspection worker failed")))?;
    let document = result.map_err(failure)?;
    execute(state, move |engine| {
        engine.finish_inspection(task, document)
    })
    .await
}

#[utoipa::path(post, path="/api/enrichment", request_body=EnrichmentRequest, responses((status=200,body=Inspection)))]
async fn enrichment(
    State(state): State<Arc<AppState>>,
    Json(input): Json<EnrichmentRequest>,
) -> Result<Json<Inspection>, ApiError> {
    let engine = state.engine.clone();
    let (document, task) = tokio::task::spawn_blocking(move || {
        let mut engine = engine
            .lock()
            .map_err(|_| anyhow::anyhow!("Media engine unavailable"))?;
        if input.cancel {
            engine.cancel_analysis(&input.id).map(|doc| (doc, None))
        } else {
            engine.analysis_task(&input.id, input.retry)
        }
    })
    .await
    .map_err(|_| failure(anyhow::anyhow!("Technical status worker failed")))?
    .map_err(failure)?;
    if let Some(task) = task {
        let id = document.id.clone();
        let generation = task.cancellation_token();
        tokio::spawn(async move {
            let Ok(_slot) = state.analysis_slots.clone().acquire_owned().await else {
                return;
            };
            let failure_engine = state.engine.clone();
            if let Err(error) = tokio::task::spawn_blocking(move || {
                let result = task.run();
                match state.engine.lock() {
                    Ok(mut engine) => engine.finish_analysis(task, result),
                    Err(error) => tracing::error!(%error, "Could not attach technical analysis"),
                }
            })
            .await
            {
                tracing::error!(%error, "Technical analysis worker failed");
                let _ = tokio::task::spawn_blocking(move || {
                    if let Ok(mut engine) = failure_engine.lock() {
                        engine.fail_analysis(
                            &id,
                            &generation,
                            "Technical worker failed; retry analysis.".into(),
                        );
                    }
                })
                .await;
            }
        });
    }
    Ok(Json(document))
}
#[utoipa::path(get,path="/api/updates",responses((status=200,body=crate::update::UpdateStatus)))]
async fn update_status(State(state): State<Arc<AppState>>) -> Json<crate::update::UpdateStatus> {
    Json(state.updates.status())
}
#[utoipa::path(post,path="/api/updates/check",responses((status=200,body=crate::update::UpdateStatus)))]
async fn update_check(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::update::UpdateStatus>, ApiError> {
    tokio::task::spawn_blocking(move || state.updates.check().map(Json).map_err(failure))
        .await
        .map_err(|_| ApiError {
            message: "Update check worker failed".into(),
        })?
}
#[utoipa::path(post,path="/api/updates/install",request_body=crate::update::UpdateRequest,responses((status=200,body=crate::update::UpdateStatus)))]
async fn update_install(
    State(state): State<Arc<AppState>>,
    Json(input): Json<crate::update::UpdateRequest>,
) -> Result<Json<crate::update::UpdateStatus>, ApiError> {
    tokio::task::spawn_blocking(move || {
        use std::sync::atomic::Ordering;
        let result = (|| -> anyhow::Result<_> {
            anyhow::ensure!(input.drafts_clear, "Save or revert drafts before updating");
            anyhow::ensure!(
                !state.desktop.closing.load(Ordering::SeqCst),
                "Tracksmith is shutting down"
            );
            anyhow::ensure!(
                state
                    .desktop
                    .updating
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok(),
                "An update is already being prepared"
            );
            let result = (|| -> anyhow::Result<_> {
                anyhow::ensure!(
                    !state.batch.busy.load(Ordering::SeqCst),
                    "Finish batch work before updating"
                );
                let batch = state.batch.view();
                anyhow::ensure!(
                    batch.phase != "review" || !batch.rows.iter().any(|r| r.status == "ready"),
                    "Save or discard the reviewed batch before updating"
                );
                anyhow::ensure!(
                    state.desktop.pending().is_empty(),
                    "Open or dismiss pending Explorer selections before updating"
                );
                let _engine = state.engine.try_lock().map_err(|_| {
                    anyhow::anyhow!("Finish the current media operation before updating")
                })?;
                state.updates.install(&input.version, &state.desktop)
            })();
            if result.is_err() {
                state.desktop.updating.store(false, Ordering::SeqCst);
            }
            result
        })();
        result.map(Json).map_err(failure)
    })
    .await
    .map_err(|_| ApiError {
        message: "Update preparation failed".into(),
    })?
}
fn failure(error: anyhow::Error) -> ApiError {
    ApiError {
        message: format!("{error:#}"),
    }
}
#[utoipa::path(get, path="/api/batch", responses((status=200,body=crate::batch::BatchView)))]
async fn batch_status(State(state): State<Arc<AppState>>) -> Json<crate::batch::BatchView> {
    Json(state.batch.view())
}
#[utoipa::path(post, path="/api/batch/clear", responses((status=200,body=crate::batch::BatchView), (status=422,body=ApiError)))]
async fn batch_clear(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::batch::BatchView>, ApiError> {
    state.batch.clear().map(Json).map_err(failure)
}
#[utoipa::path(post, path="/api/batch/preview", request_body=crate::batch::BatchRequest, responses((status=200,body=crate::batch::BatchView)))]
async fn batch_preview(
    State(state): State<Arc<AppState>>,
    Json(input): Json<crate::batch::BatchRequest>,
) -> Result<Json<crate::batch::BatchView>, ApiError> {
    if state
        .desktop
        .updating
        .load(std::sync::atomic::Ordering::SeqCst)
        || state
            .desktop
            .closing
            .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err(ApiError {
            message: "Shutting down".into(),
        });
    }
    state
        .batch
        .preview(state.engine.clone(), input)
        .map(Json)
        .map_err(failure)
}
#[utoipa::path(post, path="/api/batch/save", request_body=crate::batch::BatchSelection, responses((status=200,body=crate::batch::BatchView)))]
async fn batch_save(
    State(state): State<Arc<AppState>>,
    Json(input): Json<crate::batch::BatchSelection>,
) -> Result<Json<crate::batch::BatchView>, ApiError> {
    if state
        .desktop
        .updating
        .load(std::sync::atomic::Ordering::SeqCst)
        || state
            .desktop
            .closing
            .load(std::sync::atomic::Ordering::SeqCst)
    {
        return Err(ApiError {
            message: "Shutting down".into(),
        });
    }
    state
        .batch
        .execute(state.engine.clone(), input, state.config_dir.clone())
        .map(Json)
        .map_err(failure)
}
#[utoipa::path(post, path="/api/batch/cancel", responses((status=200,body=crate::batch::BatchView)))]
async fn batch_cancel(State(state): State<Arc<AppState>>) -> Json<crate::batch::BatchView> {
    state
        .batch
        .cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
    state.batch.discard_review();
    Json(state.batch.view())
}
#[utoipa::path(get, path="/api/batch/history", responses((status=200,body=Vec<crate::batch::BatchView>)))]
async fn batch_history(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<crate::batch::BatchView>>, ApiError> {
    let dir = state.config_dir.clone();
    tokio::task::spawn_blocking(move || {
        crate::batch::history(Path::new(&dir))
            .map(Json)
            .map_err(failure)
    })
    .await
    .map_err(|_| ApiError {
        message: "History worker failed".into(),
    })?
}
#[utoipa::path(post, path="/api/batch/pick-file", request_body=PickerRequest, responses((status=200,body=Option<String>)))]
async fn batch_pick_file(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PickerRequest>,
) -> Result<Json<Option<String>>, ApiError> {
    execute(state, move |engine| {
        crate::desktop::pick_media_file(&input.marker)?
            .map(|p| engine.grant_batch_file(&p))
            .transpose()
    })
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchRequest {
    path: Option<String>,
}
async fn desktop_launch(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<LaunchRequest>,
) -> Result<Json<bool>, StatusCode> {
    if headers
        .get("x-tracksmith-launch")
        .and_then(|v| v.to_str().ok())
        != Some(state.desktop.launch_key.as_str())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(path) = input.path {
        state
            .desktop
            .enqueue(Path::new(&path))
            .map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
    }
    Ok(Json(true))
}
#[utoipa::path(get, path="/api/desktop/pending", responses((status=200,body=Vec<crate::lifecycle::LaunchItem>)))]
async fn desktop_pending(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<crate::lifecycle::LaunchItem>> {
    Json(state.desktop.pending())
}
#[utoipa::path(post, path="/api/desktop/open", request_body=IdRequest, responses((status=200,body=Inspection)))]
async fn desktop_open(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IdRequest>,
) -> Result<Json<Inspection>, ApiError> {
    let item = state.desktop.selected(&input.id).map_err(failure)?;
    if item.folder {
        return Err(ApiError {
            message: "Choose this folder in Batch editor".into(),
        });
    }
    let desktop = state.desktop.clone();
    let id = item.id.clone();
    let task = execute(state.clone(), move |e| {
        e.selected_inspection_task(Path::new(&item.path))
    })
    .await?
    .0;
    let result = finish_open(state, task).await?;
    desktop.dismiss(&id);
    Ok(result)
}
#[utoipa::path(post, path="/api/desktop/folder", request_body=IdRequest, responses((status=200,body=DirectoryListing)))]
async fn desktop_folder(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IdRequest>,
) -> Result<Json<DirectoryListing>, ApiError> {
    let item = state.desktop.selected(&input.id).map_err(failure)?;
    if !item.folder {
        return Err(ApiError {
            message: "Choose this file in Editor".into(),
        });
    }
    let desktop = state.desktop.clone();
    execute(state, move |e| {
        let listing = e.open_selected_folder(Path::new(&item.path))?;
        desktop.dismiss(&item.id);
        Ok(listing)
    })
    .await
}
#[utoipa::path(post, path="/api/desktop/dismiss", request_body=IdRequest, responses((status=200,body=bool)))]
async fn desktop_dismiss(
    State(state): State<Arc<AppState>>,
    Json(input): Json<IdRequest>,
) -> Json<bool> {
    state.desktop.dismiss(&input.id);
    Json(true)
}
#[utoipa::path(get, path="/api/desktop/settings", responses((status=200,body=crate::desktop::Integration)))]
async fn desktop_settings() -> Json<crate::desktop::Integration> {
    Json(crate::desktop::integration_status())
}
#[utoipa::path(post, path="/api/desktop/settings", request_body=crate::desktop::Integration, responses((status=200,body=crate::desktop::Integration)))]
async fn desktop_configure(
    Json(input): Json<crate::desktop::Integration>,
) -> Result<Json<crate::desktop::Integration>, ApiError> {
    tokio::task::spawn_blocking(move || {
        crate::desktop::configure_integration(input)
            .map(Json)
            .map_err(failure)
    })
    .await
    .map_err(|_| ApiError {
        message: "Windows settings worker failed".into(),
    })?
}
#[utoipa::path(post, path="/api/desktop/stop", responses((status=200,body=bool)))]
async fn desktop_stop(State(state): State<Arc<AppState>>) -> Json<bool> {
    let engine = state.engine.clone();
    tokio::task::spawn_blocking(move || {
        if let Ok(mut engine) = engine.lock() {
            engine.cancel_all_analysis();
        }
    });
    state
        .batch
        .cancel
        .store(true, std::sync::atomic::Ordering::SeqCst);
    state.desktop.stop();
    Json(true)
}

#[utoipa::path(get, path = "/api/rules", responses((status = 200, body = RuleLibrary), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn rule_library(State(state): State<Arc<AppState>>) -> Result<Json<RuleLibrary>, ApiError> {
    rule_task(state, move |store| store.load()).await
}
#[utoipa::path(get, path = "/api/rules/standard", responses((status = 200, body = RuleLibrary)), security(("session_token" = [])))]
async fn standard_rules() -> Json<RuleLibrary> {
    Json(rules::standard_library())
}
#[utoipa::path(post, path = "/api/rules", request_body = RuleLibrary, responses((status = 200, body = RuleLibrary), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn save_rule_library(
    State(state): State<Arc<AppState>>,
    Json(library): Json<RuleLibrary>,
) -> Result<Json<RuleLibrary>, ApiError> {
    rule_task(state, move |store| store.save(library)).await
}
#[utoipa::path(post, path = "/api/rules/validate", request_body = RuleLibrary, responses((status = 200, body = RuleLibrary), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn validate_rule_library(
    State(state): State<Arc<AppState>>,
    Json(library): Json<RuleLibrary>,
) -> Result<Json<RuleLibrary>, ApiError> {
    rule_task(state, move |_| {
        rules::validate(&library)?;
        Ok(library)
    })
    .await
}

async fn rule_task<T: Send + 'static>(
    state: Arc<AppState>,
    task: impl FnOnce(&rules::RuleStore) -> anyhow::Result<T> + Send + 'static,
) -> Result<Json<T>, ApiError> {
    tokio::task::spawn_blocking(move || {
        let store = rules::RuleStore::new(Path::new(&state.config_dir));
        task(&store).map(Json).map_err(|error| ApiError {
            message: format!("{error:#}"),
        })
    })
    .await
    .map_err(|_| ApiError {
        message: "Rules worker failed; reload the library before retrying".into(),
    })?
}
#[utoipa::path(get, path = "/api/rules/catalog", responses((status = 200, body = RuleCatalog)), security(("session_token" = [])))]
async fn rule_catalog() -> Json<RuleCatalog> {
    Json(rules::catalog())
}
#[utoipa::path(post, path = "/api/rules/preview", request_body = RulePreviewRequest, responses((status = 200, body = RulePreview), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn rule_preview(
    State(state): State<Arc<AppState>>,
    Json(request): Json<RulePreviewRequest>,
) -> Result<Json<RulePreview>, ApiError> {
    execute(state, move |engine| engine.preview_rules(request)).await
}

async fn execute<T: Send + 'static>(
    state: Arc<AppState>,
    task: impl FnOnce(&mut Engine) -> anyhow::Result<T> + Send + 'static,
) -> Result<Json<T>, ApiError> {
    tokio::task::spawn_blocking(move || {
        if state
            .desktop
            .updating
            .load(std::sync::atomic::Ordering::SeqCst)
            || state.batch.busy.load(std::sync::atomic::Ordering::SeqCst)
            || state
                .desktop
                .closing
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(ApiError {
                message: "Batch work, update preparation or shutdown is in progress; wait for it to finish".into(),
            });
        }
        let mut engine = state.engine.try_lock().map_err(|_| ApiError {
            message: "A media operation is already running; wait for it to finish".into(),
        })?;
        task(&mut engine).map(Json).map_err(|error| ApiError {
            message: format!("{error:#}"),
        })
    })
    .await
    .map_err(|_| ApiError {
        message: "Media worker failed; inspect the file before retrying any save".into(),
    })?
}

#[utoipa::path(get, path = "/api/status", responses((status = 200, body = Status)), security(("session_token" = [])))]
async fn status(State(state): State<Arc<AppState>>) -> Json<Status> {
    let engine = state
        .engine_status
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Json(Status {
        name: "Tracksmith".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        phase: "Standard naming and reviewed editing".into(),
        config_dir: state.config_dir.clone(),
        media_root_count: engine.roots.len(),
        editing_available: engine.writing_available,
        engine,
    })
}
#[utoipa::path(post, path = "/api/inspect", request_body = OpenRequest, responses((status = 200, body = Inspection), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn inspect(
    State(state): State<Arc<AppState>>,
    Json(input): Json<OpenRequest>,
) -> Result<Json<Inspection>, ApiError> {
    let task = execute(state.clone(), move |engine| {
        engine.inspection_task(Path::new(&input.path))
    })
    .await?
    .0;
    finish_open(state, task).await
}
#[utoipa::path(post, path = "/api/pick-file", request_body = PickerRequest, responses((status = 200, body = Option<Inspection>), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn pick_file(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PickerRequest>,
) -> Result<Json<Option<Inspection>>, ApiError> {
    let task = execute(state.clone(), move |engine| {
        let Some(path) = crate::desktop::pick_media_file(&input.marker)? else {
            return Ok(None);
        };
        engine.selected_inspection_task(&path).map(Some)
    })
    .await?
    .0;
    match task {
        Some(task) => finish_open(state, task)
            .await
            .map(|Json(doc)| Json(Some(doc))),
        None => Ok(Json(None)),
    }
}
#[utoipa::path(post, path = "/api/browser", request_body = BrowserRequest, responses((status = 200, body = DirectoryListing), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn browse(
    State(state): State<Arc<AppState>>,
    Json(input): Json<BrowserRequest>,
) -> Result<Json<DirectoryListing>, ApiError> {
    execute(state, move |engine| {
        engine.browse_directory(
            input.path.as_deref().map(Path::new),
            input.offset,
            &input.search,
        )
    })
    .await
}
#[utoipa::path(post, path = "/api/pick-folder", request_body = PickerRequest, responses((status = 200, body = Option<DirectoryListing>), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn pick_folder(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PickerRequest>,
) -> Result<Json<Option<DirectoryListing>>, ApiError> {
    execute(state, move |engine| {
        let Some(path) = crate::desktop::pick_media_folder(&input.marker)? else {
            return Ok(None);
        };
        engine.open_selected_folder(&path).map(Some)
    })
    .await
}
#[utoipa::path(post, path = "/api/open-recent", request_body = OpenRequest, responses((status = 200, body = Inspection), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn open_recent(
    State(state): State<Arc<AppState>>,
    Json(input): Json<OpenRequest>,
) -> Result<Json<Inspection>, ApiError> {
    let task = execute(state.clone(), move |engine| {
        engine.recent_inspection_task(Path::new(&input.path))
    })
    .await?
    .0;
    finish_open(state, task).await
}
#[utoipa::path(post, path = "/api/open-location", request_body = OpenRequest, responses((status = 200, body = DirectoryListing), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn open_location(
    State(state): State<Arc<AppState>>,
    Json(input): Json<OpenRequest>,
) -> Result<Json<DirectoryListing>, ApiError> {
    execute(state, move |engine| {
        engine.open_recent_location(Path::new(&input.path))
    })
    .await
}
#[utoipa::path(post, path = "/api/favourite", request_body = FavouriteRequest, responses((status = 200, body = EngineStatus), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn favourite(
    State(state): State<Arc<AppState>>,
    Json(input): Json<FavouriteRequest>,
) -> Result<Json<EngineStatus>, ApiError> {
    let cached = state.engine_status.clone();
    execute(state, move |engine| {
        let status = engine.set_favourite(Path::new(&input.path), input.favourite)?;
        *cached.write().unwrap_or_else(|e| e.into_inner()) = status.clone();
        Ok(status)
    })
    .await
}
#[utoipa::path(post, path = "/api/plans", request_body = PlanRequest, responses((status = 200, body = Plan), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn plan(
    State(state): State<Arc<AppState>>,
    Json(input): Json<PlanRequest>,
) -> Result<Json<Plan>, ApiError> {
    execute(state, move |engine| {
        engine.plan(&input.inspection_id, input.edits)
    })
    .await
}
#[utoipa::path(post, path = "/api/save", request_body = SaveRequest, responses((status = 200, body = SaveReport), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn save(
    State(state): State<Arc<AppState>>,
    Json(input): Json<SaveRequest>,
) -> Result<Json<SaveReport>, ApiError> {
    execute(state, move |engine| engine.save(&input.plan_id)).await
}
#[utoipa::path(post, path = "/api/restore-plan", request_body = RestoreRequest, responses((status = 200, body = Plan), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn restore(
    State(state): State<Arc<AppState>>,
    Json(input): Json<RestoreRequest>,
) -> Result<Json<Plan>, ApiError> {
    execute(state, move |engine| {
        engine.restore_plan(&input.operation_id)
    })
    .await
}
#[utoipa::path(post, path = "/api/locations", request_body = LocationsRequest, responses((status = 200, body = EngineStatus), (status = 422, body = ApiError)), security(("session_token" = [])))]
async fn locations(
    State(state): State<Arc<AppState>>,
    Json(input): Json<LocationsRequest>,
) -> Result<Json<EngineStatus>, ApiError> {
    let cached = state.engine_status.clone();
    execute(state, move |engine| {
        let result = engine.set_locations(input.roots.into_iter().map(PathBuf::from).collect())?;
        *cached.write().unwrap_or_else(|e| e.into_inner()) = result.clone();
        Ok(result)
    })
    .await
}

#[derive(OpenApi)]
#[openapi(paths(status, inspect, enrichment, pick_file, browse, pick_folder, open_recent, open_location, favourite, plan, save, restore, locations, rule_library, standard_rules, save_rule_library, validate_rule_library, rule_catalog, rule_preview, batch_status, batch_clear, batch_preview, batch_save, batch_cancel, batch_history, batch_pick_file, desktop_pending, desktop_open, desktop_folder, desktop_dismiss, desktop_settings, desktop_configure, desktop_stop, update_status, update_check, update_install), components(schemas(Status, ApiError)), modifiers(&SecurityScheme))]
pub struct ApiDoc;
struct SecurityScheme;
impl utoipa::Modify for SecurityScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "session_token",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::Http::new(
                        utoipa::openapi::security::HttpAuthScheme::Bearer,
                    ),
                ),
            );
        }
    }
}
pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/updates", get(update_status))
        .route("/api/updates/check", post(update_check))
        .route("/api/updates/install", post(update_install))
        .route("/api/batch", get(batch_status))
        .route("/api/batch/clear", post(batch_clear))
        .route("/api/batch/preview", post(batch_preview))
        .route("/api/batch/save", post(batch_save))
        .route("/api/batch/cancel", post(batch_cancel))
        .route("/api/batch/history", get(batch_history))
        .route("/api/batch/pick-file", post(batch_pick_file))
        .route("/api/desktop/launch", post(desktop_launch))
        .route("/api/desktop/pending", get(desktop_pending))
        .route("/api/desktop/open", post(desktop_open))
        .route("/api/desktop/folder", post(desktop_folder))
        .route("/api/desktop/dismiss", post(desktop_dismiss))
        .route(
            "/api/desktop/settings",
            get(desktop_settings).post(desktop_configure),
        )
        .route("/api/desktop/stop", post(desktop_stop))
        .route("/api/inspect", post(inspect))
        .route("/api/enrichment", post(enrichment))
        .route("/api/pick-file", post(pick_file))
        .route("/api/browser", post(browse))
        .route("/api/pick-folder", post(pick_folder))
        .route("/api/open-recent", post(open_recent))
        .route("/api/open-location", post(open_location))
        .route("/api/favourite", post(favourite))
        .route("/api/plans", post(plan))
        .route("/api/save", post(save))
        .route("/api/restore-plan", post(restore))
        .route("/api/locations", post(locations))
        .route("/api/rules", get(rule_library).post(save_rule_library))
        .route("/api/rules/standard", get(standard_rules))
        .route("/api/rules/validate", post(validate_rule_library))
        .route("/api/rules/catalog", get(rule_catalog))
        .route("/api/rules/preview", post(rule_preview))
        .route(
            "/api/openapi.json",
            get(|| async { Json(ApiDoc::openapi()) }),
        )
        .with_state(Arc::new(state))
}
