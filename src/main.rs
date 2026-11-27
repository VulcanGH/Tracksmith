#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
use anyhow::{Context, Result};
use axum::{
    body::Body,
    extract::Request,
    http::{StatusCode, header},
    middleware,
    response::Response,
};
use clap::Parser;
use rust_embed::Embed;
use std::net::{Ipv4Addr, SocketAddr};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};
use tower_http::{limit::RequestBodyLimitLayer, trace::TraceLayer};
use tracksmith::{
    api::{self, AppState},
    config::{self, Config},
    security::{self, LocalSecurity},
};

#[derive(Parser)]
#[command(version, about = "Media metadata. Refined.")]
struct Args {
    /// Override the configured port for this session.
    #[arg(long, value_parser = clap::value_parser!(u16).range(1..))]
    port: Option<u16>,
    /// Internal, isolated MediaInfo inspection process. Not an HTTP endpoint.
    #[arg(long, num_args = 2, hide = true)]
    mediainfo_worker: Option<Vec<PathBuf>>,
    /// Keep the service running without opening a browser.
    #[arg(long)]
    background: bool,
    /// Disable the notification-area icon (automation/headless use).
    #[arg(long)]
    no_tray: bool,
    /// Open a media file or folder selected in Explorer.
    path: Option<PathBuf>,
}

#[derive(Embed)]
#[folder = "frontend/dist/"]
struct Assets;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let interactive = !args.background && args.mediainfo_worker.is_none();
    run(args).await.inspect_err(|error| {
        if interactive {
            tracksmith::desktop::show_error(&format!("Tracksmith could not start: {error:#}"));
        }
    })
}
async fn run(args: Args) -> Result<()> {
    if let Some(worker) = &args.mediainfo_worker {
        return tracksmith::media::mediainfo::worker(&worker[0], &worker[1]);
    }
    let dir = config::config_dir()?;
    let path = args.path.as_ref().map(|p| {
        if p.is_absolute() {
            p.clone()
        } else {
            std::env::current_dir().unwrap_or_default().join(p)
        }
    });
    let Some(instance) = tracksmith::lifecycle::Instance::acquire(&dir)? else {
        return tracksmith::lifecycle::forward(&dir, path.as_deref(), args.background);
    };
    let logs = dir.join("logs");
    std::fs::create_dir_all(&logs)?;
    let log_path = logs.join("tracksmith.log");
    if std::fs::metadata(&log_path).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
        let _ = std::fs::remove_file(logs.join("tracksmith.previous.log"));
        std::fs::rename(&log_path, logs.join("tracksmith.previous.log"))?;
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(Mutex::new(log))
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tracksmith=info,tower_http=info".into()),
        )
        .init();
    let config = Config::load(&dir)?;
    let env_port = std::env::var("TRACKSMITH_PORT")
        .ok()
        .map(|value| value.parse::<u16>())
        .transpose()
        .context("TRACKSMITH_PORT must be an integer between 1 and 65535")?;
    let port = args.port.or(env_port).unwrap_or(config.port);
    anyhow::ensure!(port != 0, "Port must be between 1 and 65535");
    let authority = format!("127.0.0.1:{port}");
    let origin = format!("http://{authority}");
    let token = uuid::Uuid::new_v4().to_string();
    let launch_key = uuid::Uuid::new_v4().to_string();
    let desktop = Arc::new(tracksmith::lifecycle::Desktop::new(
        format!("{origin}/#token={token}"),
        launch_key.clone(),
    ));
    if let Some(path) = path {
        desktop.enqueue(&path)?;
    }
    let batch = Arc::new(tracksmith::batch::Batch::default());
    let updates = Arc::new(tracksmith::update::Updater::new(dir.clone(), port)?);
    let engine = tracksmith::media::Engine::new(&config, dir.clone())?;
    let engine_status = engine.status();
    let app = api::router(AppState {
        config_dir: dir.display().to_string(),
        engine: Arc::new(Mutex::new(engine)),
        analysis_slots: Arc::new(tokio::sync::Semaphore::new(1)),
        engine_status: Arc::new(RwLock::new(engine_status)),
        desktop: desktop.clone(),
        batch: batch.clone(),
        updates,
    })
    .fallback(assets)
    .layer(RequestBodyLimitLayer::new(512 * 1024))
    .layer(tower::limit::ConcurrencyLimitLayer::new(32))
    .layer(middleware::from_fn_with_state(
        LocalSecurity {
            authority,
            origin: origin.clone(),
            token: token.clone(),
        },
        security::guard,
    ))
    .layer(TraceLayer::new_for_http());
    let listener = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
        .context("Binding loopback service; check whether Tracksmith is already running or choose --port")?;
    tracing::info!(address = %origin, "Tracksmith ready");
    instance.publish(&tracksmith::lifecycle::Session {
        port,
        token: token.clone(),
        launch_key,
    })?;
    let tray = if args.no_tray {
        None
    } else {
        Some(tracksmith::desktop::start_tray(
            desktop.clone(),
            batch.clone(),
            dir.clone(),
        ))
    };
    // Deliberately outside tracing: the session credential must not enter routine logs.
    println!("Open Tracksmith: {origin}/#token={token}");
    println!("Closing the browser leaves Tracksmith running. Exit from the tray or Settings.");
    if !args.background {
        tracksmith::desktop::open_url(&desktop.url)?;
    }
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            tokio::select! { result=tokio::signal::ctrl_c()=>{ if let Err(error)=result{tracing::error!(%error,"Signal handler failed");} }, _=desktop.shutdown.notified()=>{} }
            desktop.closing.store(true,std::sync::atomic::Ordering::SeqCst);
            batch.cancel.store(true,std::sync::atomic::Ordering::SeqCst);
            while batch.busy.load(std::sync::atomic::Ordering::SeqCst) { batch.cancel.store(true,std::sync::atomic::Ordering::SeqCst); tokio::time::sleep(std::time::Duration::from_millis(100)).await; }
            tracing::info!("Shutting down");
        })
        .await?;
    if let Some(tray) = tray {
        let _ = tray.join();
    }
    Ok(())
}

async fn assets(request: Request) -> Response {
    if !matches!(
        *request.method(),
        axum::http::Method::GET | axum::http::Method::HEAD
    ) {
        return Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::empty())
            .unwrap();
    }
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(path) {
        Some(asset) => Response::builder()
            .header(
                header::CONTENT_TYPE,
                mime_guess::from_path(path).first_or_octet_stream().as_ref(),
            )
            .body(if request.method() == axum::http::Method::HEAD {
                Body::empty()
            } else {
                Body::from(asset.data.into_owned())
            })
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    }
}
