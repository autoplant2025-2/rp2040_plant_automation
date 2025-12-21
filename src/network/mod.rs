use alloc::rc::Rc;
use cyw43::Control;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::clocks::RoscRng;
use embassy_rp::Peri;
use embassy_rp::peripherals::{DMA_CH2, PIN_23, PIN_24, PIN_25, PIN_29, PIO0};
use embassy_rp::pio::{Common, Irq, StateMachine};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;use crate::config_manager::SharedConfig;
use crate::network::wifi::{init_wifi, SharedWifiControl};
use crate::time_manager::SharedTimeManager;
use portable_atomic::AtomicBool;

mod time_sync_task;
mod connection_monitor;
pub mod wifi;
pub mod http_server;

pub type ShareNetworkStack = Rc<Mutex<NoopRawMutex, Stack<'static>>>;

pub static WIFI_AUTOCONNECT_ENABLED: AtomicBool = AtomicBool::new(true);

pub async fn init_network(
	spawner: &Spawner,
	config: SharedConfig,
	time_manager: SharedTimeManager,
    shared_sensor_data: crate::sensor_manager::SharedSensorData,
    shared_history: crate::sensor_history::SharedHistory,

	wifi_pio_common: &mut Common<'static, PIO0>,
	wifi_sm: StateMachine<'static, PIO0, 1>,
	wifi_irq: Irq<'static, PIO0, 0>,
	wifi_pwr: Peri<'static, PIN_23>,
	wifi_cs: Peri<'static, PIN_25>,
	wifi_dio: Peri<'static, PIN_24>,
	wifi_clk: Peri<'static, PIN_29>,
	wifi_dma: Peri<'static, DMA_CH2>,
) -> (SharedWifiControl, ShareNetworkStack) {
	let (control, net_driver) = init_wifi(
		spawner,
		wifi_pio_common,
		wifi_sm,
		wifi_irq,
		wifi_pwr,
		wifi_cs,
		wifi_dio,
		wifi_clk,
		wifi_dma,
	).await;

	let net_config = Config::dhcpv4(Default::default());
	//let config = embassy_net::Config::ipv4_static(embassy_net::StaticConfigV4 {
	//    address: Ipv4Cidr::new(Ipv4Address::new(192, 168, 69, 2), 24),
	//    dns_servers: Vec::new(),
	//    gateway: Some(Ipv4Address::new(192, 168, 69, 1)),
	//});

	let mut rng = RoscRng;

	// Generate random seed
	let seed = rng.next_u64();

	// Init network stack
	static RESOURCES: StaticCell<StackResources<4>> = StaticCell::new();
	let (stack, runner) = embassy_net::new(net_driver, net_config, RESOURCES.init(StackResources::new()), seed);

	spawner.spawn(net_task(runner).unwrap());

	let shared_stack = Rc::new(Mutex::new(stack));
	
	spawner.spawn(time_sync_task::time_sync_task(time_manager, shared_stack.clone(), config.clone()).unwrap());
	spawner.spawn(connection_monitor::connection_monitor_task(control.clone(), shared_stack.clone(), config.clone()).unwrap());
    spawner.spawn(http_server::http_server_task(shared_stack.clone(), config.clone(), shared_sensor_data.clone(), shared_history.clone()).unwrap());

	(control, shared_stack)
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
	runner.run().await
}