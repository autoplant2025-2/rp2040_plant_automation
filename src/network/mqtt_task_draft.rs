use embassy_net::{Stack, IpListenEndpoint};
use embassy_time::{Duration, Timer};
use minimq::{Minimq, Publication, QoS, Retain};
use crate::config_manager::SharedConfig;
use crate::sensor_manager::SharedSensorData;
use crate::network::ShareNetworkStack;
use alloc::vec::Vec;
use serde::Serialize;
use crate::config_types::PlantConfiguration;
use num_traits::Float;

const MQTT_BROKER_IP: [u8; 4] = [192, 168, 0, 100]; // Default fallback, should be configurable? or discoverable?

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

#[embassy_executor::task]
pub async fn mqtt_task(
    stack: ShareNetworkStack,
    config: SharedConfig,
    sensor_data: SharedSensorData,
) {
    let mut rx_buffer = [0u8; 1024];
    let mut tx_buffer = [0u8; 1024];

    loop {
        // Wait for link up
        {
            let s = stack.lock().await;
            if !s.is_link_up() || !s.is_config_up() {
                drop(s);
                Timer::after(Duration::from_secs(1)).await;
                continue;
            }
        }
        
        let stack_handle = {
             let s = stack.lock().await;
             *s
        };

        // Get Gateway IP as a "best guess" for the Broker, or use hardcoded/config
        // For now, let's assume the PC running the broker is the Gateway (common in shared net) 
        // OR we just hardcode it to a known likely IP, or user config.
        // Let's rely on the Config for the Broker IP?
        // Actually, let's try to connect to the Gateway IP first as a smart default for "Host PC connection sharing"
        let broker_ip = if let Some(v4) = stack_handle.config_v4() {
             if let Some(gateway) = v4.gateway {
                 gateway.octets() // Connect to Gateway (PC)
             } else {
                 [192, 168, 137, 1] // Fallback common Windows Hotspot IP
             }
        } else {
             Timer::after(Duration::from_secs(5)).await;
             continue;
        };

        // Construct Minimq Client
        let mut mqtt: Minimq<'_, _, _, minimq::broker::IpBroker> = Minimq::new(
            stack_handle,
            &mut tx_buffer,
            &mut rx_buffer,
            minimq::Config::new(
                minimq::broker::IpBroker::new(embassy_net::IpAddress::Ipv4(embassy_net::Ipv4Address::from_bytes(&broker_ip))),
                "rp2040-plant"
            )
            .keepalive_interval(60)
        );

        defmt::info!("MQTT: Connecting to Broker at {:?}", broker_ip);

        // Connection Loop
        loop {
            if mqtt.client().is_connected() {
                 match mqtt.poll(|client, topic, message, properties| {
                     match topic {
                         "plant/config" => {
                             defmt::info!("MQTT: Config Update Received");
                             // Handle Config JSON
                             // This part is tricky because we can't lock async mutex in this closure easily if it blocks
                             // But minimq poll is sync? 
                             // We should probably just parse it here and then signal updates.
                             if let Ok(json_str) = core::str::from_utf8(message) {
                                 // We need to pass this out.
                                 // For now, let's just log it. 
                                 // Ideally we use a Channel to send the config update to the main loop context
                             }
                         },
                         _ => {}
                     }
                 }) {
                     Ok(_) => {},
                     Err(e) => {
                         defmt::warn!("MQTT Poll Error: {:?}", e);
                         // Likely disconnected
                         break; 
                     }
                 }
                 
                 // Subscribe if needed (only once)
                 // minimq doesn't track subscription state automatically perfectly across reconnects? 
                 // We should subscribe when we detect connection.
            } else {
                // Not connected
                 if !mqtt.client().is_connected() {
                      // Attempt Reconnect
                      // Minimq autoconnects on poll if configured? 
                      // Actually we just poll.
                 }
            }
            
            // Wait a bit
            Timer::after(Duration::from_millis(100)).await;
        }

        Timer::after(Duration::from_secs(5)).await;
        // Re-create client and retry
    }
}
