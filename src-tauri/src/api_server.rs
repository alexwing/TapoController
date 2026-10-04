//! Embedded local API server (M3) for future plugins (VLC/PotPlayer ambilight,
//! music sync, ...). REST for discrete commands + a WebSocket `/stream` for
//! high-rate color frames. Supports multi-bulb targeting and synchronized mode.

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
    bulb_id: Option<String>,
}
#[derive(Deserialize)]
struct BrightnessBody {
    value: u8,
    bulb_id: Option<String>,
}
#[derive(Deserialize)]
struct ColorBody {
    hue: Option<u16>,
    saturation: Option<u8>,
    r: Option<u8>,
    g: Option<u8>,
    b: Option<u8>,
    bulb_id: Option<String>,
}
#[derive(Deserialize)]
struct TempBody {
    kelvin: u16,
    bulb_id: Option<String>,
}
#[derive(Deserialize)]
struct WhiteBody {
    kelvin: u16,
    brightness: u8,
    bulb_id: Option<String>,
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
#[derive(Deserialize)]
struct SyncBody {
    sync: bool,
}
#[derive(Deserialize)]
struct ActiveBody {
    target: String,
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

async fn get_bulbs(State(s): State<SharedSvc>) -> impl IntoResponse {
    let service = svc(&s).await;
    let states = service.get_all_states().await;
    let sync = service.sync_all();
    let target = service.active_target().await;
    Json(json!({
        "ok": true,
        "bulbs": states,
        "sync_all": sync,
        "active_target": target
    }))
}

async fn set_sync(
    State(s): State<SharedSvc>,
    Json(b): Json<SyncBody>,
) -> impl IntoResponse {
    svc(&s).await.set_sync_all(b.sync);
    ok().into_response()
}

async fn set_active(
    State(s): State<SharedSvc>,
    Json(b): Json<ActiveBody>,
) -> impl IntoResponse {
    svc(&s).await.set_active_target(b.target).await;
    ok().into_response()
}

async fn power(
    State(s): State<SharedSvc>,
    Json(b): Json<PowerBody>,
) -> impl IntoResponse {
    let service = svc(&s).await;
    let res = if let Some(ref id) = b.bulb_id {
        service.set_bulb_power(id, b.on).await
    } else {
        service.set_power(b.on).await
    };
    match res {
        Ok(()) => ok().into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn brightness(
    State(s): State<SharedSvc>,
    Json(b): Json<BrightnessBody>,
) -> impl IntoResponse {
    let service = svc(&s).await;
    let res = if let Some(ref id) = b.bulb_id {
        service.set_bulb_brightness(id, b.value).await
    } else {
        service.set_brightness(b.value).await
    };
    match res {
        Ok(()) => ok().into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn color(
    State(s): State<SharedSvc>,
    Json(b): Json<ColorBody>,
) -> impl IntoResponse {
    let service = svc(&s).await;
    let res = match (b.hue, b.saturation, b.r, b.g, b.b, b.bulb_id) {
        (Some(h), Some(sa), .., Some(id)) => service.set_bulb_hue_saturation(&id, h, sa).await,
        (Some(h), Some(sa), .., None) => service.set_hue_saturation(h, sa).await,
        (_, _, Some(r), Some(g), Some(bl), Some(id)) => service.set_bulb_rgb(&id, r, g, bl).await,
        (_, _, Some(r), Some(g), Some(bl), None) => service.set_rgb(r, g, bl).await,
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
    let service = svc(&s).await;
    let res = if let Some(ref id) = b.bulb_id {
        service.set_bulb_color_temp(id, b.kelvin).await
    } else {
        service.set_color_temp(b.kelvin).await
    };
    match res {
        Ok(()) => ok().into_response(),
        Err(e) => err(e).into_response(),
    }
}

async fn white(
    State(s): State<SharedSvc>,
    Json(b): Json<WhiteBody>,
) -> impl IntoResponse {
    let service = svc(&s).await;
    let res = if let Some(ref id) = b.bulb_id {
        service.set_bulb_white(id, b.kelvin, b.brightness).await
    } else {
        service.set_white(b.kelvin, b.brightness).await
    };
    match res {
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

/// Each text frame is `{"r":..,"g":..,"b":..}`. Frames go to the service coalescing pipeline.
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
        .route("/bulbs", get(get_bulbs))
        .route("/sync", post(set_sync))
        .route("/active", post(set_active))
        .route("/state", get(get_state))
        .route("/power", post(power))
        .route("/brightness", post(brightness))
        .route("/color", post(color))
        .route("/color_temp", post(color_temp))
        .route("/white", post(white))
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
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let body: serde_json::Value = reqwest::get("http://127.0.0.1:7799/health")
            .await
            .expect("request")
            .json()
            .await
            .expect("json");
        assert_eq!(body["ok"], true);
        assert_eq!(body["service"], "tapo-controller");

        let resp = reqwest::Client::new()
            .post("http://127.0.0.1:7799/power")
            .json(&serde_json::json!({ "on": true }))
            .send()
            .await
            .expect("send");
        assert_eq!(resp.status().as_u16(), 502);
    }
}
