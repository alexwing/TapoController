//! Embedded local API server (M3) for future plugins (VLC/PotPlayer ambilight,
//! music sync, ...). REST for discrete commands + a WebSocket `/stream` for
//! high-rate color frames. It shares the exact same `ControlService` as the
//! desktop UI, so rate-limiting/back-pressure are unified.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tapo_proto::ControlService;
use tower_http::cors::CorsLayer;

use crate::SharedSvc;

async fn svc(s: &SharedSvc) -> Arc<ControlService> {
    s.lock().await.clone()
}

fn err(e: tapo_proto::TapoError) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_GATEWAY,
        Json(json!({ "ok": false, "error": e.to_string() })),
    )
}
fn ok() -> Json<Value> {
    Json(json!({ "ok": true }))
}

#[derive(Deserialize)]
struct PowerBody {
    on: bool,
}
#[derive(Deserialize)]
struct BrightnessBody {
    value: u8,
}
#[derive(Deserialize)]
struct ColorBody {
    hue: Option<u16>,
    saturation: Option<u8>,
    r: Option<u8>,
    g: Option<u8>,
    b: Option<u8>,
}
#[derive(Deserialize)]
struct TempBody {
    kelvin: u16,
}
#[derive(Deserialize)]
struct Frame {
    r: u8,
    g: u8,
    b: u8,
}
#[derive(Deserialize)]
struct AnimBody {
    on: bool,
    speed: Option<u8>,
}

async fn health() -> impl IntoResponse {
    Json(json!({ "ok": true, "service": "tapo-controller", "api": 1 }))
}

async fn monitors() -> impl IntoResponse {
    Json(json!({ "ok": true, "monitors": crate::ambilight::list_monitors() }))
}

async fn get_state(State(s): State<SharedSvc>) -> impl IntoResponse {
    match svc(&s).await.get_state().await {
        Ok(info) => Json(json!({ "ok": true, "state": info })).into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn power(
    State(s): State<SharedSvc>,
    Json(b): Json<PowerBody>,
) -> impl IntoResponse {
    match svc(&s).await.set_power(b.on).await {
        Ok(()) => ok().into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn brightness(
    State(s): State<SharedSvc>,
    Json(b): Json<BrightnessBody>,
) -> impl IntoResponse {
    match svc(&s).await.set_brightness(b.value).await {
        Ok(()) => ok().into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn color(
    State(s): State<SharedSvc>,
    Json(b): Json<ColorBody>,
) -> impl IntoResponse {
    let service = svc(&s).await;
    let res = match (b.hue, b.saturation, b.r, b.g, b.b) {
        (Some(h), Some(sa), ..) => service.set_hue_saturation(h, sa).await,
        (_, _, Some(r), Some(g), Some(bl)) => service.set_rgb(r, g, bl).await,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "ok": false, "error": "provide {hue,saturation} or {r,g,b}" })),
            )
                .into_response()
        }
    };
    match res {
        Ok(()) => ok().into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn color_temp(
    State(s): State<SharedSvc>,
    Json(b): Json<TempBody>,
) -> impl IntoResponse {
    match svc(&s).await.set_color_temp(b.kelvin).await {
        Ok(()) => ok().into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn animation(
    State(s): State<SharedSvc>,
    Json(b): Json<AnimBody>,
) -> impl IntoResponse {
    let service = svc(&s).await;
    if b.on {
        if let Err(e) = service.set_power(true).await {
            return err(e).into_response();
        }
    }
    service.set_animation(b.on, b.speed);
    ok().into_response()
}

async fn stream_ws(
    State(s): State<SharedSvc>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_stream(socket, s))
}

/// Each text frame is `{"r":..,"g":..,"b":..}`. We never block the socket:
/// frames are handed to the service's coalescing pipeline, which drops
/// intermediate frames and rate-limits to the device's capabilities.
async fn handle_stream(mut socket: WebSocket, s: SharedSvc) {
    let service = svc(&s).await;
    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Text(t) => {
                if let Ok(f) = serde_json::from_str::<Frame>(&t) {
                    service.submit_color(f.r, f.g, f.b);
                }
            }
            Message::Binary(d) if d.len() >= 3 => {
                service.submit_color(d[0], d[1], d[2]);
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

pub async fn serve(bind: String, port: u16, shared: SharedSvc) -> std::io::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/monitors", get(monitors))
        .route("/state", get(get_state))
        .route("/power", post(power))
        .route("/brightness", post(brightness))
        .route("/color", post(color))
        .route("/color_temp", post(color_temp))
        .route("/animation", post(animation))
        .route("/stream", get(stream_ws))
        .layer(CorsLayer::permissive())
        .with_state(shared);

    let addr: SocketAddr = format!("{bind}:{port}")
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], port)));
    tracing::info!("API server listening on http://{addr}");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tapo_proto::{ControlService, DeviceConfig, StreamConfig};
    use tokio::sync::Mutex;

    #[tokio::test]
    async fn api_server_boots_and_serves_health() {
        let svc: SharedSvc = Arc::new(Mutex::new(ControlService::new(
            DeviceConfig::default(),
            StreamConfig::default(),
        )));
        tokio::spawn(serve("127.0.0.1".into(), 7799, svc));
        // Give the listener a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let body: serde_json::Value = reqwest::get("http://127.0.0.1:7799/health")
            .await
            .expect("request")
            .json()
            .await
            .expect("json");
        assert_eq!(body["ok"], true);
        assert_eq!(body["service"], "tapo-controller");

        // A control call with no credentials must fail gracefully (502), not panic.
        let resp = reqwest::Client::new()
            .post("http://127.0.0.1:7799/power")
            .json(&serde_json::json!({ "on": true }))
            .send()
            .await
            .expect("send");
        assert_eq!(resp.status().as_u16(), 502);
    }
}
