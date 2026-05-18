use crate::args::HtmlConfig;
use axum::Extension;
//use axum::middleware::from_extractor;
use axum_client_ip::ClientIpSource;
use std::net::SocketAddr;
use tokio::sync::watch;

const DIST_INDEX_HTML_GZ: &[u8] = include_bytes!("../../dist/index.html.gz");

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

pub async fn run(config: HtmlConfig) -> anyhow::Result<Option<watch::Sender<()>>> {
    if config.enable {
        let (tx, shutdown_receiver) = watch::channel(());
        let listener = tokio::net::TcpListener::bind(&config.listen).await?;
        let router = axum::Router::new()
            .route("/", axum::routing::get(dist_index_gz))
            //.layer(from_extractor::<middleware::IpValidator>())
            .layer(ClientIpSource::ConnectInfo.into_extension())
            .layer(Extension(config.allow))
            .fallback(handler_404);
        tokio::spawn(async move {
            tracing::info!("http server started on {:?}", config.listen);
            let res = axum::serve(
                listener,
                router.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown(shutdown_receiver))
            .await;
            if let Err(e) = res {
                tracing::error!("http server error: {}", e);
            }
        });
        Ok(Some(tx))
    } else {
        Ok(None)
    }
}
