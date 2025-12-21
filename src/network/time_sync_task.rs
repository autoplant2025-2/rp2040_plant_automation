use embassy_time::{Duration, Timer};
use crate::time_manager::SharedTimeManager;
use crate::network::ShareNetworkStack;
use crate::config_manager::SharedConfig;

#[embassy_executor::task]
pub async fn time_sync_task(time_manager: SharedTimeManager, stack: ShareNetworkStack, config: SharedConfig) {

    loop {
        // Scope for stack lock
        let stack_handle = {
            let lock = stack.lock().await;
            (*lock).clone() 
        };

        if time_manager.sync_time(stack_handle, 0).await.is_ok() {
            if let Some(time) = time_manager.get_time() {
                 let mut cfg = config.lock().await;
                 cfg.update_settings(|s| {
                     s.last_datetime = time.timestamp() as u64;
                 }).await;
                 defmt::info!("Time synced and saved to flash.");
            }
        }
        
		Timer::after(Duration::from_secs(60 * 60 * 24)).await;

    }
}
