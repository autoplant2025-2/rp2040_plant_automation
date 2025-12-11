
use embassy_time::{Duration, Timer};
use crate::config_manager::SharedConfig;

#[embassy_executor::task]
pub async fn time_sync_task(config: SharedConfig) {
	loop {
		Timer::after(Duration::from_secs(60)).await;

		defmt::info!("Running scheduled time sync...");

		let mut cfg = config.lock().await;
		cfg.update_settings(|s| {
			s.last_datetime += 60; // Increment mock
		}).await;

		defmt::info!("Time synced and saved.");
	}
}
