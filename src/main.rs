#![no_std]
#![no_main]
extern crate alloc;

#[macro_export]
macro_rules! modpub {
    ($name:ident) => {
        mod $name;
        pub use $name::*;
    };
}


//defmt
use defmt_rtt as _;

//panic handling
use panic_probe as _;
use alloc::boxed::Box;
use core::mem::MaybeUninit;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::{ClockConfig, CoreVoltage};
use embassy_rp::config::Config;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::rotary_encoder::{PioEncoder, PioEncoderProgram};

//pub use defmt::;
use embassy_time::Timer;
use embedded_hal_async::delay::DelayNs;
use slint::ComponentHandle;
use talc::{ClaimOnOom, Talc, Talck};
use crate::config_manager::init_persistence_config;

mod main_ui;
pub mod control;
pub mod sensor_manager;
pub mod hardware_manager;
pub mod userscript;
pub mod persistence_manager;
pub mod config_manager;
pub mod config_types;
pub mod time_manager;
mod ui;
mod network;

static mut ARENA: MaybeUninit<[u8; 1024 * 160]> = MaybeUninit::uninit();

// 힙 allocator
#[allow(static_mut_refs)]
#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> = Talc::new(unsafe {
	ClaimOnOom::new(talc::Span::from_array(ARENA.as_ptr().cast_mut()))
}).lock();

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<embassy_rp::peripherals::I2C0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
	let mut cc = ClockConfig::system_freq(280_000_000).unwrap(); //오버클럭의 생활화.....
	cc.core_voltage = CoreVoltage::V1_30;
	let p = embassy_rp::init(Config::new(cc));

	let shared_config = init_persistence_config(
		p.FLASH, p.DMA_CH1
	).await;

	//spawn input handling task
	let Pio {
		mut common, sm0, ..
	} = Pio::new(p.PIO0, Irqs);



	// pio encoder
	let prg = PioEncoderProgram::new(&mut common);
	let encoder0 = PioEncoder::new(&mut common, sm0, p.PIN_14, p.PIN_15, &prg);
	
	ui::init_ui(
		&spawner,
		shared_config.clone(),
		// &mut common,
		// sm0,
		// p.PIN_14,
		// p.PIN_15,
		encoder0,
		p.PIN_8,
		
		p.SPI0,
		p.PIN_6,
		p.PIN_7,
		p.DMA_CH0
	);
	
	network::init_network(&spawner, shared_config.clone());

	loop {
		Timer::after_secs(10).await;
		let mut buf = [0u8; 256];
		let mut w = &mut buf[0..];
		use embedded_io::Write;
		writeln!(&mut w, "{:?}", ALLOCATOR.lock().get_counters()).ok();
		defmt::error!("{}", alloc::format!("{:#?}", str::from_utf8(&buf).unwrap()));
	}
}


// embassy time의 api를 사용한 딜레이 구현
pub struct EmbassyDelay;

impl DelayNs for EmbassyDelay {
	async fn delay_ns(&mut self, ns: u32) {
		Timer::after_nanos(ns as u64).await
	}
}
