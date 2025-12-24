use embassy_net::{IpEndpoint};
use embassy_time::{Duration, Timer, Instant};
use crate::config_manager::SharedConfig;
use crate::sensor_manager::SharedSensorData;
use crate::network::ShareNetworkStack;
use serde::{Deserialize, Serialize};
use embassy_net::tcp::TcpSocket;
use myrtio_mqtt::MqttClient;
use myrtio_mqtt::transport::TcpTransport;
use myrtio_mqtt::MqttOptions;
use myrtio_mqtt::QoS;
use myrtio_mqtt::MqttEvent;


// Placeholder for myrtio-mqtt
// use myrtio_mqtt::*; 

#[derive(Serialize)]
struct MqttLog {
    timestamp: u64,
    temp_in: f32,
    hum_in: u8,
    temp_out: f32,
    hum_out: u8,
    soil: f32,
    ec: f32,
}

#[derive(Deserialize)]
struct ConfigUpdate {
    target_temp: Option<f32>,
    plant_name: Option<alloc::string::String>,
    light_intensity: Option<u8>,
    light_start_hour: Option<u8>,
    light_end_hour: Option<u8>,
}

#[embassy_executor::task]
pub async fn mqtt_task(
    stack: ShareNetworkStack,
    config: SharedConfig,
    sensor_data: SharedSensorData,
) {
    let mut rx_buffer = [0u8; 1024];
    let mut tx_buffer = [0u8; 1024];
    
    // Hardcoded Broker IP (PC)
    let broker_ip = embassy_net::Ipv4Address::new(192, 168, 0, 12);

    loop {
        Timer::after(Duration::from_secs(2)).await;

        let mut socket = TcpSocket::new(stack.lock().await.clone(), &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));
        
        defmt::info!("MQTT: Connecting to {}...", broker_ip);

        // Connect
        if let Err(e) = socket.connect(IpEndpoint::new(embassy_net::IpAddress::Ipv4(broker_ip), 1883)).await {
            defmt::warn!("MQTT: Connect failed: {:?}", defmt::Debug2Format(&e));
            continue;
        }
        
        defmt::info!("MQTT: Connected TCP");

        // PROBE myrtio_mqtt Client
        // let mut mqtt_buf = [0u8; 1024];
        // let mut client = Client::new(socket, &mut mqtt_buf);
        // We will comment this out or try it? 
        // Let's TRY it to get error.

        // Assuming typical signature: new(socket, buffer) or similar.
        // Or maybe just new(options)?
        // I will guess: new(socket, buffer).
        
        let transport = TcpTransport::new(socket, Duration::from_secs(30));
        let options = MqttOptions::new("rp2040-plant");
        let mut client: MqttClient<'_, _, 5, 1024> = MqttClient::new(transport, options);
        
        defmt::info!("MQTT: Client Created. Connecting...");
        
        if let Err(e) = client.connect().await {
            defmt::warn!("MQTT: Connect Error: {:?}", defmt::Debug2Format(&e));
            continue;
        }
        defmt::info!("MQTT: Connected!");

        if let Err(e) = client.subscribe("plant/config", QoS::AtLeastOnce).await {
             defmt::warn!("MQTT: Subscribe Error: {:?}", defmt::Debug2Format(&e));
             // continue? or retry?
        } else {
             defmt::info!("MQTT: Subscribed.");
        }
        

        
        let mut last_publish = Instant::now();

        // Loop
        loop {
             match client.poll().await {
                Ok(Some(MqttEvent::Publish(pkt))) => {
                    defmt::info!("MQTT: Received Publish on {}", pkt.topic);
                    if pkt.topic == "plant/config" {
                         if let Ok(json_str) = core::str::from_utf8(&pkt.payload) {
                             if let Ok(update) = serde_json::from_str::<ConfigUpdate>(json_str) {
                                 defmt::info!("MQTT: Applying Config Update");
                                 let mut cfg = config.lock().await;
                                 cfg.update_plant_config(|c| {
                                     if let Some(v) = update.target_temp { c.target_temp = v; }
                                     if let Some(v) = update.light_intensity { c.light_intensity = v; }
                                     if let Some(v) = update.light_start_hour { c.light_start_hour = v; }
                                     if let Some(v) = update.light_end_hour { c.light_end_hour = v; }
                                     if let Some(v) = update.plant_name { 
                                         if let Ok(s) = heapless::String::try_from(v.as_str()) {
                                              c.plant_name = s;
                                         }
                                     }
                                 }).await;
                             }
                         }
                    }
                },

                Ok(None) => {},
                Err(e) => {
                    defmt::warn!("MQTT: Poll Error: {:?}", defmt::Debug2Format(&e));
                    break;
                }
             }

             if last_publish.elapsed().as_secs() >= 5 {
                 // Get Sensor Data
                 let log_entry = {
                     let data = sensor_data.lock().await;
                     let ts = if let Ok(cfg) = config.try_lock() {
                         cfg.settings().last_datetime
                     } else { 0 };

                     MqttLog {
                         timestamp: ts,
                         temp_in: data.internal.map(|r| r.temp.to_num()).unwrap_or(0.0),
                         hum_in: data.internal.map(|r| r.hum).unwrap_or(0),
                         temp_out: data.external.map(|r| r.temp.to_num()).unwrap_or(0.0),
                         hum_out: data.external.map(|r| r.hum).unwrap_or(0),
                         soil: data.soil_moisture.map(|v| v.to_num()).unwrap_or(0.0),
                         ec: data.ec_level.map(|v| v.to_num()).unwrap_or(0.0),
                     }
                 };
                 
                 // Serialize
                 if let Ok(json) = serde_json::to_string(&log_entry) {
                     client.publish("plant/logs", json.as_bytes(), QoS::AtMostOnce).await.ok();
                 }
                 last_publish = Instant::now();
             }
             
             Timer::after(Duration::from_secs(5)).await;
        }
        

    }
}
