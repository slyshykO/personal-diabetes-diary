use crate::args::HtmlConfig;
use crate::state;
use axum::{Extension, Json, routing::get};
//use axum::middleware::from_extractor;
use axum_client_ip::ClientIpSource;
use serde::Serialize;
use std::net::SocketAddr;
use tokio::sync::watch;

const DIST_INDEX_HTML_GZ: &[u8] = include_bytes!("../../dist/index.html.gz");

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn shutdown(mut rx: watch::Receiver<()>) {
    match rx.changed().await {
        Ok(()) => {
            tracing::info!("http server shutdown");
        }
        Err(e) => {
            tracing::error!("wait channel is closed.{}", e);
        }
    }
}

#[allow(clippy::unused_async)]
async fn dist_index_gz() -> impl axum::response::IntoResponse {
    (
        [("Content-Type", "text/html"), ("Content-Encoding", "gzip")],
        DIST_INDEX_HTML_GZ,
    )
}

#[allow(clippy::unused_async)]
async fn handler_404() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        "404. nothing to see here",
    )
}

#[allow(clippy::unused_async)]
async fn api_health() -> impl axum::response::IntoResponse {
    Json(HealthResponse {
        status: "ok",
        version: crate::args::get_version_str(),
    })
}

pub async fn run(
    config: HtmlConfig,
    app_state: state::AppState,
) -> anyhow::Result<Option<watch::Sender<()>>> {
    if config.enable {
        let (tx, shutdown_receiver) = watch::channel(());
        let listener = tokio::net::TcpListener::bind(&config.listen).await?;
        let api_router = axum::Router::new()
            .route("/", get(api_health))
            .route("/health", get(api_health))
            .fallback(handler_404);
        let router = axum::Router::new()
            .nest("/api", api_router)
            .route("/", get(dist_index_gz))
            .route("/{*path}", get(dist_index_gz))
            //.layer(from_extractor::<middleware::IpValidator>())
            .layer(ClientIpSource::ConnectInfo.into_extension())
            .layer(Extension(config.allow))
            .fallback(handler_404);
        app_state.spawn("http server", async move {
            tracing::info!("http server started on {:?}", config.listen);
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown(shutdown_receiver))
            .await
            .map_err(Into::into)
        })?;
        Ok(Some(tx))
    } else {
        Ok(None)
    }
}
