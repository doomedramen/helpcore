#![allow(missing_docs)]

use axum::{
    Router,
    body::Body,
    http::{Method, StatusCode, Uri, header},
    response::Response,
};
use tower_http::services::{ServeDir, ServeFile};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded_web.rs"));
}

/// Attaches the web UI (embedded or external directory) as a fallback service.
pub fn attach(router: Router, enabled: bool) -> Router {
    if !enabled {
        tracing::info!("web UI disabled");
        return router;
    }

    if let Some(web_dir) = external_web_dir() {
        tracing::info!(path = %web_dir.display(), "serving web UI from directory");
        return router.fallback_service(
            ServeDir::new(&web_dir).not_found_service(ServeFile::new(web_dir.join("index.html"))),
        );
    }

    if embedded::FILES.is_empty() {
        tracing::warn!(
            "web UI requested but no assets are embedded; rebuild with `make build` or use --headless"
        );
        return router;
    }

    tracing::info!(files = embedded::FILES.len(), "serving embedded web UI");
    router.fallback(serve_embedded)
}

/// Returns `true` when embedded web assets were compiled in.
pub fn embedded_available() -> bool {
    !embedded::FILES.is_empty()
}

fn external_web_dir() -> Option<std::path::PathBuf> {
    let configured = std::env::var_os("HELPCORE_WEB_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "/usr/local/share/helpcore/web".into());
    configured.is_dir().then_some(configured)
}

async fn serve_embedded(method: Method, uri: Uri) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return not_found();
    }

    let Some((path, contents)) = resolve_asset(uri.path()) else {
        return not_found();
    };
    let content_type = mime_guess::from_path(path).first_or_octet_stream();
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(contents)
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type.as_ref())
        .header(header::CONTENT_LENGTH, contents.len())
        .header(
            header::CACHE_CONTROL,
            if path.starts_with("_next/static/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            },
        )
        .body(body)
        .expect("valid embedded asset response")
}

fn resolve_asset(request_path: &str) -> Option<(&'static str, &'static [u8])> {
    let requested = request_path.trim_start_matches('/');
    let direct = if requested.is_empty() {
        "index.html".to_string()
    } else {
        requested.to_string()
    };
    find_asset(&direct)
        .or_else(|| find_asset(&format!("{}/index.html", direct.trim_end_matches('/'))))
        .or_else(|| {
            (!has_file_extension(&direct))
                .then(|| find_asset("index.html"))
                .flatten()
        })
}

fn find_asset(path: &str) -> Option<(&'static str, &'static [u8])> {
    embedded::FILES
        .binary_search_by_key(&path, |(asset_path, _)| *asset_path)
        .ok()
        .map(|index| embedded::FILES[index])
}

fn has_file_extension(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
}

fn not_found() -> Response {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .expect("valid not found response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_only_build_has_no_embedded_assets() {
        if std::env::var_os("HELPCORE_EMBED_WEB_DIR").is_none() {
            assert!(!embedded_available());
        }
    }

    #[test]
    fn detects_asset_paths() {
        assert!(has_file_extension("_next/static/app.js"));
        assert!(has_file_extension("favicon.ico"));
        assert!(!has_file_extension("login"));
        assert!(!has_file_extension(""));
    }
}
