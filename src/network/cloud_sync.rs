use embassy_time::{Duration, Timer};
use crate::config_manager::SharedConfig;
use crate::sensor_manager::SharedSensorData;
use crate::network::ShareNetworkStack;
use embassy_net::{Stack, tcp::TcpSocket, dns::DnsSocket, dns::DnsQueryType};
use embedded_tls::{TlsConnection, TlsConfig, TlsContext, Aes128GcmSha256};
use embedded_io_async::{Read, Write};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use embassy_rp::clocks::RoscRng;
use serde::{Serialize, Deserialize};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

const FIREBASE_HOST: &str = "plantdatabase-1adf2-default-rtdb.asia-southeast1.firebasedatabase.app";
const FIREBASE_URL: &str = "https://plantdatabase-1adf2-default-rtdb.asia-southeast1.firebasedatabase.app";

#[derive(Serialize)]
struct LogEntry {
    temp: f32,
    hum: u8,
    soil: f32,
    ec: f32,
    uptime_ms: u64,
}

#[derive(Deserialize)]
struct RemoteConfig {
    target_temp: Option<f32>,
    light_intensity: Option<u8>,
}

#[embassy_executor::task]
pub async fn cloud_sync_task(
    shared_stack: ShareNetworkStack,
    shared_config: SharedConfig,
    shared_sensor: SharedSensorData,
) {
    let mut rng = RoscRng;
    let seed = rng.next_u64();
    let mut chacha_rng = ChaCha8Rng::seed_from_u64(seed);

    loop {
        Timer::after(Duration::from_secs(10)).await;

        let stack = {
             let guard = shared_stack.lock().await;
             *guard
        };
        
        if stack.is_link_up() {
            // 1. Download Settings
            match make_request(&stack, &mut chacha_rng, "GET", "/settings.json", None).await {
                Ok(response_body) => {
                     if let Ok(remote) = serde_json::from_str::<RemoteConfig>(&response_body) {
                           let mut cfg = shared_config.lock().await;
                           cfg.update_plant_config(|pc| {
                               if let Some(t) = remote.target_temp { pc.target_temp = t; }
                               if let Some(l) = remote.light_intensity { pc.light_intensity = l; }
                           }).await;
                           defmt::info!("Cloud Sync: Settings updated");
                     }
                },
                Err(e) => defmt::warn!("Cloud Sync: GET settings failed: {:?}", defmt::Debug2Format(&e)),
            }

            // 2. Upload Logs
             let (temp, hum, soil, ec) = {
                 let s = shared_sensor.lock().await;
                 let t = s.internal.map(|r| r.temp.to_num::<f32>()).unwrap_or(0.0);
                 let h = s.internal.map(|r| r.hum).unwrap_or(0);
                 let sl = s.soil_moisture.map(|r| r.to_num::<f32>()).unwrap_or(0.0);
                 let e = s.ec_level.map(|r| r.to_num::<f32>()).unwrap_or(0.0);
                 (t, h, sl, e)
             };
             
             let log = LogEntry {
                 temp, hum, soil, ec,
                 uptime_ms: embassy_time::Instant::now().as_millis(),
             };
             
             if let Ok(json) = serde_json::to_string(&log) {
                 match make_request(&stack, &mut chacha_rng, "POST", "/logs.json", Some(&json)).await {
                     Ok(_) => defmt::info!("Cloud Sync: Logs uploaded"),
                     Err(e) => defmt::warn!("Cloud Sync: Upload failed: {:?}", defmt::Debug2Format(&e)),
                 }
             }
        }
    }
}

#[derive(Debug)]
enum HttpError {
    Network(embassy_net::tcp::ConnectError),
    Dns,
    Tls(embedded_tls::TlsError),
    Io(embedded_io::ErrorKind),
    HttpFormat,
}

impl From<embassy_net::tcp::ConnectError> for HttpError { fn from(e: embassy_net::tcp::ConnectError) -> Self { Self::Network(e) } }
impl From<embedded_tls::TlsError> for HttpError { fn from(e: embedded_tls::TlsError) -> Self { Self::Tls(e) } }
impl From<embedded_io::ErrorKind> for HttpError { fn from(e: embedded_io::ErrorKind) -> Self { Self::Io(e) } }

async fn make_request(
    stack: &Stack<'static>,
    rng: &mut ChaCha8Rng,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<String, HttpError> {
    // Buffers
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    
    let mut socket = TcpSocket::new(*stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));

    let dns = DnsSocket::new(*stack);
    let ip_addrs = dns.query(FIREBASE_HOST, DnsQueryType::A).await.map_err(|_| HttpError::Dns)?;
    if ip_addrs.is_empty() { return Err(HttpError::Dns); }
    let remote_endpoint = (ip_addrs[0], 443);

    socket.connect(remote_endpoint).await.map_err(|e| HttpError::Network(e))?;

    // Increased read buffer to 16KB for large TLS records/chains
    let mut read_record_buffer = alloc::vec![0; 16640];
    let mut write_record_buffer = [0; 4096];
    
    let tls_config = TlsConfig::<Aes128GcmSha256>::new().with_server_name(FIREBASE_HOST);
    let mut tls = TlsConnection::new(socket, &mut read_record_buffer, &mut write_record_buffer);

    // Insecure (NoVerify)
    tls.open::<_, embedded_tls::NoVerify>(TlsContext::new(&tls_config, rng)).await.map_err(HttpError::Tls)?;

    // Build Request
    let body_len = body.map(|b| b.len()).unwrap_or(0);
    // Note: heapless::String size? Path can be long?
    // Using alloc::format! involves heap but we have an allocator.
    let request_header = format!(
        "{} {} HTTP/1.1\r\nHost: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        method, path, FIREBASE_HOST, body_len
    );

    tls.write_all(request_header.as_bytes()).await.map_err(HttpError::Tls)?;
    if let Some(b) = body {
        tls.write_all(b.as_bytes()).await.map_err(HttpError::Tls)?;
    }
    tls.flush().await.map_err(HttpError::Tls)?;

    // Read Response
    // Simple parser: wait for \r\n\r\n, then read body.
    // NOTE: This assumes body comes after headers and we read enough.
    // We'll read into a separate buffer or reuse something?
    // We can just read chunk by chunk.
    
    let mut response_buf = [0u8; 2048];
    let len = tls.read(&mut response_buf).await.map_err(HttpError::Tls)?;
    
    // Check for \r\n\r\n
    // Find double newline
    let data = &response_buf[..len];
    let mut body_start = 0;
    
    // Very naive search
    for i in 0..len.saturating_sub(4) {
        if &data[i..i+4] == b"\r\n\r\n" {
            body_start = i + 4;
            break;
        }
    }
    
    if body_start == 0 {
        // Maybe header too long?
        return Err(HttpError::HttpFormat);
    }
    
    // Status Code?
    // HTTP/1.1 200 OK
    // Check if 200?
    // Skip verification for now, just return body.
    
    let body_str = core::str::from_utf8(&data[body_start..]).unwrap_or("");
    // If chunked, this is broken. Firebase usually returns simple JSON for GET if small.
    // For POST, body usually empty or simple JSON.
    
    Ok(String::from(body_str))
}
