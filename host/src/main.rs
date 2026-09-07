use anyhow::Context;
use axum::body::Body;
use axum::extract::Request;
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use axum::Router;
use bytes::Bytes;
use std::net::SocketAddr;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

const HOP: &[&str] = &[
    "connection",
    "keep-alive",
    "proxy-authenticate",
    "proxy-authorization",
    "te",
    "trailers",
    "transfer-encoding",
    "upgrade",
    "host",
    "content-length",
];

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let dist = ui_dist()?;
    let index = dist.join("index.html");
    let http = reqwest::Client::builder()
        .use_rustls_tls()
        .build()
        .context("http client")?;

    let static_files = ServeDir::new(&dist).not_found_service(ServeFile::new(&index));
    let app = Router::new()
        .route(
            "/xai-auth/{*path}",
            any({
                let http = http.clone();
                move |req| proxy(http.clone(), "https://auth.x.ai", "/xai-auth", req)
            }),
        )
        .route(
            "/xai-api/{*path}",
            any({
                let http = http.clone();
                move |req| proxy(http.clone(), "https://api.x.ai", "/xai-api", req)
            }),
        )
        .fallback_service(static_files);

    let bind = std::env::var("FUN_SITE_BIND").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let addr: SocketAddr = bind.parse().context("FUN_SITE_BIND")?;
    println!("fun-site  http://{addr}/");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn ui_dist() -> anyhow::Result<PathBuf> {
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for p in [
        here.join("../dist"),
        PathBuf::from("dist"),
        here.join("dist"),
    ] {
        if p.join("index.html").is_file() {
            return Ok(p.canonicalize().unwrap_or(p));
        }
    }
    anyhow::bail!("build the site first: trunk build --release")
}

async fn proxy(http: reqwest::Client, origin: &str, prefix: &str, req: Request) -> Response {
    let path = req.uri().path();
    let rest = path.strip_prefix(prefix).unwrap_or(path);
    let rest = if rest.is_empty() { "/" } else { rest };
    let mut url = format!("{origin}{rest}");
    if let Some(q) = req.uri().query() {
        url.push('?');
        url.push_str(q);
    }

    let method = req.method().clone();
    let headers = req.headers().clone();
    let body = match axum::body::to_bytes(req.into_body(), 8 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            return (StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };

    let mut builder = http.request(to_method(&method), &url);
    for (name, value) in headers.iter() {
        if HOP.iter().any(|h| name.as_str() == *h) {
            continue;
        }
        builder = builder.header(name.as_str(), value.as_bytes());
    }
    if !body.is_empty() {
        builder = builder.body(body.to_vec());
    }

    match builder.send().await {
        Ok(resp) => relay(resp).await,
        Err(e) => (StatusCode::BAD_GATEWAY, e.to_string()).into_response(),
    }
}

fn to_method(m: &Method) -> reqwest::Method {
    reqwest::Method::from_bytes(m.as_str().as_bytes()).unwrap_or(reqwest::Method::POST)
}

async fn relay(resp: reqwest::Response) -> Response {
    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    for (name, value) in resp.headers() {
        if HOP.iter().any(|h| name.as_str() == *h) {
            continue;
        }
        if let (Ok(n), Ok(v)) = (
            HeaderName::from_bytes(name.as_str().as_bytes()),
            HeaderValue::from_bytes(value.as_bytes()),
        ) {
            headers.append(n, v);
        }
    }
    let bytes = resp.bytes().await.unwrap_or_else(|_| Bytes::new());
    let mut out = Response::new(Body::from(bytes));
    *out.status_mut() = status;
    *out.headers_mut() = headers;
    if !out.headers().contains_key(header::CONTENT_TYPE) {
        out.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    out
}
