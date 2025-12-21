use embassy_time::{Duration, Timer};
use cyw43::JoinOptions;
use crate::config_manager::SharedConfig;
use crate::network::wifi::SharedWifiControl;
use crate::network::ShareNetworkStack;

#[embassy_executor::task]
pub async fn connection_monitor_task(
    wifi: SharedWifiControl,
    stack: ShareNetworkStack,
    config: SharedConfig,
) {
    loop {
        Timer::after(Duration::from_secs(30)).await;

        if !crate::network::WIFI_AUTOCONNECT_ENABLED.load(portable_atomic::Ordering::Relaxed) {
            continue;
        }

        let link_up = {
            let stack = stack.lock().await;
            stack.is_link_up()
        };

        if link_up {
            continue;
        }

        let (ssid, pass) = {
            let cfg = config.lock().await;
            let s = cfg.settings();
            if s.wifi_ssid.is_empty() {
                continue;
            }
            (s.wifi_ssid.clone(), s.wifi_password.clone())
        };

        defmt::info!("WiFi disconnected. Attempting to reconnect to {}", ssid.as_str());

        let mut wifi = wifi.lock().await;
        
        // Re-check link status or if we are already joined at driver level?
        // But embassy-net link status is the high level truth.
        
        let options = if let Some(pass) = &pass {
            JoinOptions::new(pass.as_bytes())
        } else {
            JoinOptions::new_open()
        };

        match wifi.join(ssid.as_str(), options).await {
            Ok(_) => defmt::info!("Reconnected successfully"),
            Err(e) => defmt::warn!("Reconnect failed: {:?}", e),
        }
    }
}
