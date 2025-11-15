#![no_std]
#![no_main]
extern crate alloc;

//defmt
use defmt_rtt as _;

//panic handling
use panic_probe as _;

slint::include_modules!();
use alloc::boxed::Box;
use core::mem::MaybeUninit;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::{ClockConfig, CoreVoltage};
use embassy_rp::config::Config;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::pio_programs::rotary_encoder::{PioEncoder, PioEncoderProgram};
use crate::lcd_task::lcd_task2;

//pub use defmt::;
use embassy_time::Timer;
use embedded_hal_async::delay::DelayNs;
use slint::ComponentHandle;
use talc::{ClaimOnOom, Talc, Talck};
use crate::encoder_button_task::{encoder_button_task, encoder_task};
use crate::keyboard::keyboard;
use crate::lcd_backend::LcdBackend;

mod lcd_task;
mod lcd_backend;
mod main_ui;
mod encoder_button_task;
mod keyboard;

static mut ARENA: MaybeUninit<[u8; 1024 * 160]> = MaybeUninit::uninit();

// 힙 allocator
#[allow(static_mut_refs)]
#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> = Talc::new(unsafe {
	ClaimOnOom::new(talc::Span::from_array(ARENA.as_ptr().cast_mut()))
}).lock();

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
	let mut cc = ClockConfig::system_freq(280_000_000).unwrap(); //오버클럭의 생활화.....
	cc.core_voltage = CoreVoltage::V1_30;
	let p = embassy_rp::init(Config::new(cc));
	//lcd backend start
	let lcd_backend = Box::new(LcdBackend::default());
	let lcd_window = lcd_backend.window.clone();
	let lcd_keyboard_window = lcd_backend.keyboard_window.clone();
	lcd_window.set_size(slint::PhysicalSize::new(128, 64));
	lcd_keyboard_window.set_size(slint::PhysicalSize::new(128, 41));
	lcd_keyboard_window.set_minimized(true);
	slint::platform::set_platform(lcd_backend).unwrap();

	//spawn lcd task
	spawner.spawn(lcd_task2(
		p.SPI1,
		p.PIN_14,
		p.PIN_15,
		p.DMA_CH0,
		lcd_window.clone(),
		lcd_keyboard_window.clone()
	).unwrap());

	//spawn input handling task
	let Pio {
		mut common, sm0, ..
	} = Pio::new(p.PIO0, Irqs);

	let prg = PioEncoderProgram::new(&mut common);
	let encoder0 = PioEncoder::new(&mut common, sm0, p.PIN_0, p.PIN_1, &prg);

	spawner.spawn(encoder_task(
		lcd_window.clone(),
		lcd_keyboard_window.clone(),
		encoder0
	).unwrap());
	spawner.spawn(encoder_button_task(
		lcd_window.clone(),
		lcd_keyboard_window.clone(),
		p.PIN_2
	).unwrap());

	//start ui
	let ui = EmbeddedUI::new().unwrap();
	ui.show().unwrap();

	let ui2 = KeyboardWindow::new().unwrap();
	ui2.show().unwrap();

	keyboard(&ui, &ui2);
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
