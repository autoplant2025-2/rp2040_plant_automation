use alloc::rc::Rc;
use cyw43::{BssInfo, Control, NetDriver};
use cyw43_pio::{PioSpi, DEFAULT_CLOCK_DIVIDER};
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::Peri;
use embassy_rp::peripherals::{DMA_CH2, PIN_23, PIN_24, PIN_25, PIN_29, PIO0};
use embassy_rp::pio::{Common, Irq, StateMachine};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use static_cell::StaticCell;

pub type SharedWifiControl = Rc<Mutex<NoopRawMutex, Control<'static>>>;

const FW: &[u8] = include_bytes!("../../embassy/cyw43-firmware/43439A0.bin");
const CLM: &[u8] = include_bytes!("../../embassy/cyw43-firmware/43439A0_clm.bin");
pub async fn init_wifi(
	spawner: &Spawner,
	pio_common: &mut Common<'static, PIO0>,
	sm: StateMachine<'static, PIO0, 1>,
	irq: Irq<'static, PIO0, 0>,
	pwr: Peri<'static, PIN_23>,
	cs: Peri<'static, PIN_25>,
	dio: Peri<'static, PIN_24>,
	clk: Peri<'static, PIN_29>,
	dma: Peri<'static, DMA_CH2>
) -> (SharedWifiControl, NetDriver<'static>) {

	let pwr = Output::new(pwr, Level::Low);
	let cs = Output::new(cs, Level::High);
	let spi = PioSpi::new(pio_common, sm, DEFAULT_CLOCK_DIVIDER, irq, cs, dio, clk, dma);


	static STATE: StaticCell<cyw43::State> = StaticCell::new();
	let state = STATE.init(cyw43::State::new());

	let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, FW).await;

	spawner.spawn(wifi_task(runner).unwrap());
	control.init(CLM).await.unwrap();
	control
		.set_power_management(cyw43::PowerManagementMode::PowerSave)
		.await.unwrap();
	(Rc::new(Mutex::new(control)), net_device)
}

#[embassy_executor::task]
async fn wifi_task(
	runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 1, DMA_CH2>>,
) -> ! {
	runner.run().await
}

#[derive(Debug, PartialEq)]
pub enum WifiSecurity {
	NoSecurity, // Open network
	Password,   // WEP, WPA, WPA2, WPA3 (Encrypted)
	Other,      // Ad-Hoc (IBSS) or non-standard modes
}

pub fn get_wifi_security(bss: &BssInfo) -> WifiSecurity {
	// Capability Bitmasks (802.11 standard)
	//const CAP_ESS: u16 = 0x0001;     // Infrastructure Mode (Access Point)
	const CAP_IBSS: u16 = 0x0002;    // Ad-Hoc Mode (Device to Device)
	const CAP_PRIVACY: u16 = 0x0010; // Privacy (Encryption Required)

	let cap = bss.capability;

	// Check for Ad-Hoc networks first (IBSS)
	// We categorize these as "Other" because they aren't standard Access Points.
	if (cap & CAP_IBSS) != 0 {
		return WifiSecurity::Other;
	}

	// Check for Privacy Bit
	// If bit 4 is set, the network expects encryption (Password).
	if (cap & CAP_PRIVACY) != 0 {
		return WifiSecurity::Password;
	}

	// If not Ad-Hoc and no Privacy bit, it's an Open network.
	WifiSecurity::NoSecurity
}