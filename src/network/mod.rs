use embassy_executor::Spawner;
use crate::config_manager::SharedConfig;

mod time_sync_task;

pub fn init_network(
	spawner: &Spawner,
	config: SharedConfig
) {
	spawner.spawn(time_sync_task::time_sync_task(config).unwrap())
}