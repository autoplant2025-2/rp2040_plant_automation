use alloc::vec;
use alloc::vec::Vec;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use picoserve::io::Read;
use picoserve::response::IntoResponse;
use picoserve::routing::{get, post};
use subtle::ConstantTimeEq;
use crate::config_manager::SharedConfig;
use crate::network::ShareNetworkStack;
use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use core::fmt::Write;

pub type AppRouter = impl picoserve::routing::PathRouter;

pub struct AppState {
    pub config: SharedConfig,
}

const HTML_HEAD: &str = r#"
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>Plant Automation Config</title>
    <style>
        body { font-family: sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; }
        label { display: block; margin-top: 10px; }
        input, textarea { width: 100%; box-sizing: border-box; }
        button { margin-top: 20px; padding: 10px 20px; }
    </style>
</head>
<body>
    <h1>Configuration</h1>
"#;

const HTML_FOOT: &str = r#"
</body>
</html>
"#;

async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.config.lock().await;
    let plant_conf = &cfg.plant_conf;
    
    let mut script_source_str = String::new();
    if let Ok(s) = core::str::from_utf8(&plant_conf.script_source) {
        script_source_str.push_str(s);
    }

    let mut html = String::new();
    html.push_str(HTML_HEAD);
    
    let _ = write!(html, r#"
    <form action="/config" method="post">
        <label for="plant_name">Plant Name:</label>
        <input type="text" id="plant_name" name="plant_name" value="{}">
        
        <label for="nominal_ec">Nominal EC:</label>
        <input type="number" step="0.1" id="nominal_ec" name="nominal_ec" value="{}">
        
        <label for="script_source">Script Source:</label>
        <textarea id="script_source" name="script_source" rows="10">{}</textarea>
        
        <button type="submit">Save</button>
    </form>
    "#, plant_conf.plant_name.as_str(), plant_conf.nominal_ec, script_source_str);
    
    html.push_str(HTML_FOOT);
    
    picoserve::response::Html(html)
}

#[derive(serde::Deserialize)]
struct ConfigForm {
    plant_name: String,
    nominal_ec: f32,
    script_source: String,
}

async fn post_config(State(state): State<AppState>, picoserve::extract::Form(form): picoserve::extract::Form<ConfigForm>) -> impl IntoResponse {
    let mut cfg = state.config.lock().await;
    
    cfg.update_plant_conf(|c| {
        c.plant_name = heapless::String::try_from(form.plant_name.as_str()).unwrap_or_default();
        c.nominal_ec = form.nominal_ec;
        
        c.script_source.clear();
        c.script_source.extend_from_slice(form.script_source.as_bytes()).ok();
    }).await;
    
    picoserve::response::Redirect::to("/")
}

use picoserve::extract::State;

pub fn make_app(config: SharedConfig) -> picoserve::Router<AppRouter, AppState> {
    picoserve::Router::new()
        .state(AppState { config })
        .route("/", get(get_config))
        .route("/config", post(post_config))
}

#[embassy_executor::task]
pub async fn http_server_task(
    stack: ShareNetworkStack,
    config: SharedConfig,
) {
    let mut rx_buffer = [0; 1024];
    let mut tx_buffer = [0; 1024];

    let app = make_app(config);

    let config = picoserve::Config::new(picoserve::Timeouts {
        start_read_request: Some(embassy_time::Duration::from_secs(5)),
        read_request: Some(embassy_time::Duration::from_secs(1)),
        write_response: Some(embassy_time::Duration::from_secs(1)),
    })
    .keep_connection_alive();

    loop {
        let stack = stack.lock().await; // Lock to clone? No, we need reference or handle.
        // Stack is Rc<Mutex<Stack>>. To use it with picoserve, we probably need a raw reference or loop accepting sockets.
        // Picoserve usually works with `serve`.
        
        // Wait, stack usage in embassy-net usually involves creating a socket.
        // We need to drop the lock to let other tasks allow network processing if `stack` is the *stack*.
        // `ShareNetworkStack` is `Rc<Mutex<NoopRawMutex, Stack<'static>>>`.
        
        // We cannot hold the stack lock indefinitely.
        // Instead, we should assume `stack` is shared via inner mutability handled by the driver/runner?
        // Ah, `Stack` in embassy-net needs to be mutable to create sockets?
        // Actually, `Stack` methods like `accept` are on `TcpSocket`.
        
        // Let's look at `embassy-net` examples.
        // Typically:
        // let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
        // loop { if let Ok(socket.accept(80).await) { serve... } }
    }
}
