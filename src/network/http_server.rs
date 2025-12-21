use alloc::format;
use alloc::string::String;
use alloc::string::ToString;
use core::fmt::Write;
use core::str::FromStr;

use picoserve::extract::State;
use picoserve::response::{IntoResponse, Response, StatusCode};
use picoserve::routing::{get, post};
use picoserve::{Router, Config, Timeouts, Server};
use embassy_time::Duration;

use crate::config_manager::SharedConfig;
use crate::network::ShareNetworkStack;
use crate::sensor_manager::SharedSensorData;

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
        .row { display: flex; align-items: center; gap: 10px; }
    </style>
    <script src="/script.js?v=3"></script>
</head>
<body>
    <h1>Configuration</h1>
"#;

const SCRIPT_CONTENT: &str = r#"
async function fetchEC() {
    try {
        const response = await fetch('/api/ec');
        const text = await response.text();
        if (text) {
            document.getElementById('nominal_ec').value = text;
        }
    } catch (e) {
        alert('Failed to read sensor');
    }
}

async function readTrayTo(id) {
    try {
        const response = await fetch('/api/tray');
        const text = await response.text();
        if (text) {
             document.getElementById(id).value = text;
        }
    } catch (e) {
        alert('Failed to read sensor');
    }
}
"#;

const HTML_FOOT: &str = r#"
</body>
</html>
"#;

use percent_encoding::percent_decode_str;

fn percent_decode(input: &str) -> String {
    percent_decode_str(input)
        .decode_utf8()
        .unwrap_or_else(|_| input.into())
        .replace("+", " ")
}

fn html_escape(input: &str) -> String {
    input.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\"", "&quot;")
}

#[derive(Clone)]
struct AppState {
    config: SharedConfig,
    sensor_data: SharedSensorData,
}

async fn index(State(state): State<AppState>) -> impl IntoResponse {
    let cfg = state.config.lock().await;
    let plant_conf = cfg.plant_config();
    let cal = cfg.calibration();
    let tray_no: i32 = cal.pid_config.water_cal_no_tray.to_num();
    let tray_dry: i32 = cal.pid_config.water_cal_dry_tray.to_num();

    
    let tray_wet: i32 = cal.pid_config.water_cal_wet_tray.to_num();
    
    // Removed script_source reading

    let mut response_buffer = String::new();
    response_buffer.push_str(HTML_HEAD);
    let _ = write!(response_buffer, r#"
    <form action="/config" method="post">
        <label for="plant_name">Plant Name:</label>
        <input type="text" id="plant_name" name="plant_name" value="{}">
        
        <label for="nominal_ec">Nominal EC:</label>
        <div class="row">
            <input type="number" step="0.01" id="nominal_ec" name="nominal_ec" value="{}">
            <button type="button" onclick="fetchEC()">Read Sensor</button>
        </div>

        <label for="target_temp">Target Temp (C):</label>
        <input type="number" step="0.1" id="target_temp" name="target_temp" value="{}">

        <label for="light_intensity">Light Intensity (0-255):</label>
        <input type="number" step="1" min="0" max="255" id="light_intensity" name="light_intensity" value="{}">

        <label>Light Schedule (Hour of Day):</label>
        <div class="row">
            <label for="light_start_hour">Start:</label>
            <input type="number" step="1" min="0" max="23" id="light_start_hour" name="light_start_hour" value="{}">
            <label for="light_end_hour">End:</label>
            <input type="number" step="1" min="0" max="23" id="light_end_hour" name="light_end_hour" value="{}">
        </div>
        
        <hr>
        <h3>Water Tray Calibration (ADC Raw 0-4095)</h3>
        
        <label for="water_cal_no_tray">No Tray Reference (Read Sensor):</label>
        <div class="row">
            <input type="number" step="1" min="0" max="4095" id="water_cal_no_tray" name="water_cal_no_tray" value="{}">
            <button type="button" onclick="readTrayTo('water_cal_no_tray')">Read</button>
        </div>
        
        <label for="water_cal_dry_tray">Dry Tray Reference (Read Sensor):</label>
        <div class="row">
            <input type="number" step="1" min="0" max="4095" id="water_cal_dry_tray" name="water_cal_dry_tray" value="{}">
            <button type="button" onclick="readTrayTo('water_cal_dry_tray')">Read</button>
        </div>
        
        <label for="water_cal_wet_tray">Wet Tray Reference (Read Sensor):</label>
        <div class="row">
            <input type="number" step="1" min="0" max="4095" id="water_cal_wet_tray" name="water_cal_wet_tray" value="{}">
            <button type="button" onclick="readTrayTo('water_cal_wet_tray')">Read</button>
        </div>
        <hr>

        <hr>
        
        <button type="submit">Save</button>
    </form>
    "#, 
    html_escape(&plant_conf.plant_name), 
    plant_conf.nominal_ec, 
    plant_conf.target_temp,
    plant_conf.light_intensity,
    plant_conf.light_start_hour,
    plant_conf.light_end_hour,
    tray_no,
    tray_dry,
    tray_wet);
    response_buffer.push_str(HTML_FOOT);

    Response::new(StatusCode::OK, response_buffer)
        .with_headers([("Content-Type", "text/html")])
}

async fn script() -> impl IntoResponse {
    Response::new(StatusCode::OK, SCRIPT_CONTENT)
        .with_headers([
            ("Content-Type", "application/javascript"),
            ("Cache-Control", "no-cache")
        ])
}

async fn get_ec(State(state): State<AppState>) -> impl IntoResponse {
    let ec_val = {
        let data = state.sensor_data.lock().await;
            if let Some(ec) = data.ec_level {
                // Convert Fixed point to float string
                let val: f32 = ec.to_num();
                format!("{:.2}", val)
        } else {
            String::from("0.0")
        }
    };
    Response::new(StatusCode::OK, ec_val)
        .with_headers([("Content-Type", "text/plain")])
}

async fn get_tray(State(state): State<AppState>) -> impl IntoResponse {
    let tray_val = {
        let data = state.sensor_data.lock().await;
            if let Some(val) = data.soil_moisture {
                format!("{:.0}", val)
        } else {
            String::from("0")
        }
    };
    Response::new(StatusCode::OK, tray_val)
        .with_headers([("Content-Type", "text/plain")])
}

async fn update_config(State(state): State<AppState>, body: String) -> impl IntoResponse {
    let mut plant_name = String::new();
    let mut nominal_ec = 0.0;
    // Removed script_source
    let mut target_temp = 25.0;
    let mut light_intensity = 0;
    let mut light_start_hour = 8;
    let mut light_end_hour = 20;
    
    // Water Tray Calibration (Option to track if they were present in form)
    let mut tray_no: Option<i32> = None;
    let mut tray_dry: Option<i32> = None;
    let mut tray_wet: Option<i32> = None;

    for pair in body.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            let decoded_value = percent_decode(value);
            match key {
                "plant_name" => plant_name = decoded_value,
                "nominal_ec" => nominal_ec = f32::from_str(&decoded_value).unwrap_or(0.0),
                "target_temp" => target_temp = f32::from_str(&decoded_value).unwrap_or(25.0),
                "light_intensity" => light_intensity = u8::from_str(&decoded_value).unwrap_or(0),
                "light_start_hour" => light_start_hour = u8::from_str(&decoded_value).unwrap_or(8),
                "light_end_hour" => light_end_hour = u8::from_str(&decoded_value).unwrap_or(20),
                "water_cal_no_tray" => tray_no = i32::from_str(&decoded_value).ok(),
                "water_cal_dry_tray" => tray_dry = i32::from_str(&decoded_value).ok(),
                "water_cal_wet_tray" => tray_wet = i32::from_str(&decoded_value).ok(),
                _ => {}
            }
        }
    }

    {
        let mut cfg = state.config.lock().await;
        
        // Update Plant Config
        cfg.update_plant_config(|c| {
            c.plant_name = heapless::String::try_from(plant_name.as_str()).unwrap_or_default();
            c.nominal_ec = nominal_ec;
            c.target_temp = target_temp;
            c.light_intensity = light_intensity;
            c.light_start_hour = light_start_hour;
            c.light_end_hour = light_end_hour;
        }).await;
        
        // Update Calibration (Water Tray)
        if tray_no.is_some() || tray_dry.is_some() || tray_wet.is_some() {
             use fixed::types::I16F16;
             cfg.update_calibration(|cal| {
                 if let Some(v) = tray_no { cal.pid_config.water_cal_no_tray = I16F16::from_num(v); }
                 if let Some(v) = tray_dry { cal.pid_config.water_cal_dry_tray = I16F16::from_num(v); }
                 if let Some(v) = tray_wet { cal.pid_config.water_cal_wet_tray = I16F16::from_num(v); }
             }).await;
        }
    }

    Response::new(StatusCode::SEE_OTHER, "")
        .with_headers([("Location", "/")])
}

#[embassy_executor::task]
pub async fn http_server_task(
    stack: ShareNetworkStack,
    shared_config: SharedConfig,
    shared_sensor_data: SharedSensorData,
) {
    let app = Router::new()
        .route("/", get(index))
        .route("/script.js", get(script))
        .route("/api/ec", get(get_ec))
        .route("/api/tray", get(get_tray))
        .route("/config", post(update_config))
        .with_state(AppState {
            config: shared_config,
            sensor_data: shared_sensor_data,
        });

    let timeouts = Timeouts {
        start_read_request: Some(Duration::from_secs(5)),
        persistent_start_read_request: Some(Duration::from_secs(60)),
        read_request: Some(Duration::from_secs(10)),
        write: Some(Duration::from_secs(10)),
    };

    let config = Config::new(timeouts);

    // Retrieve the stack handle
    let stack_handle = {
        let s = stack.lock().await;
        *s
    };

    let mut buffer = [0u8; 2048];
    let mut tcp_rx = [0u8; 128];
    let mut tcp_tx = [0u8; 128];

    let server = Server::new(&app, &config, &mut buffer);
    server.listen_and_serve(0, stack_handle, 80, &mut tcp_rx, &mut tcp_tx).await;
}
