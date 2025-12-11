use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::Peri;
use embassy_rp::peripherals::{PIN_23, PIN_25, PIN_29};
use crate::config_manager::SharedConfig;

mod time_sync_task;
mod wifi;

pub fn init_network(
	spawner: &Spawner,
	config: SharedConfig,

	wifi_pwr: Peri<'static, PIN_23>,
	wifi_cs: Peri<'static, PIN_25>,
	wifi_tx_rx: Peri<'static, PIN_29>
) {
	let wifi_pwr = Output::new(wifi_pwr, Level::Low);
	let wifi_cs = Output::new(wifi_cs, Level::High);
	let mut pio = Pio::new(p.PIO0);
	let spi = PioSpi::new(&mut pio.common, pio.sm0, pio.irq0, cs, p.PIN_24, p.PIN_29, p.DMA_CH0);
	spawner.spawn(time_sync_task::time_sync_task(config).unwrap())
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
	stack.run().await
}