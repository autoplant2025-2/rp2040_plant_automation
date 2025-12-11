use cyw43_pio::PioSpi;
use embassy_rp::gpio::Output;
use embassy_rp::peripherals::{DMA_CH1, PIN_23, PIN_25, PIO0};

#[embassy_executor::task]
async fn wifi_task(
	runner: cyw43::Runner<'static, Output<'static, PIN_23>, PioSpi<'static, PIN_25, PIO0, 0, DMA_CH1>>,
) -> ! {
	runner.run().await
}