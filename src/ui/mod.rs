mod encoder_button_input_task;
mod encoder_input_task;
mod keyboard;
mod lcd_backend;
mod lcd_task;
mod initial_configuration;

use alloc::boxed::Box;
use embassy_executor::Spawner;
use embassy_rp::Peri;
use embassy_rp::peripherals::{DMA_CH0, PIN_14, PIN_15, PIN_6, PIN_7, PIN_8, PIO0, SPI0};
use embassy_rp::pio::{Common, StateMachine};
use embassy_rp::pio_programs::rotary_encoder::{PioEncoder, PioEncoderProgram};
use encoder_button_input_task::encoder_button_input_task;
use encoder_input_task::encoder_input_task;
use keyboard::keyboard;
use lcd_backend::LcdBackend;
use lcd_task::lcd_task2;
use crate::config_manager::SharedConfig;
use crate::ui::initial_configuration::initial_configuration_ui_task;

slint::include_modules!();

pub fn init_ui(
	spawner: &Spawner,
	config: SharedConfig,
	// # hardwares
	// ## input hardwares
	//pio_encoder: PioEncoder<'static, PIO0, 0>,
	pio: &mut Common<'static, PIO0>,
	pio_sm: StateMachine<'static, PIO0, 0>,
	enc_a: Peri<'static, PIN_14>,
	enc_b: Peri<'static, PIN_15>,
	enc_button: Peri<'static, PIN_8>,
	// ## display hardwares
	spi: Peri<'static, SPI0>,
	sclk: Peri<'static, PIN_6>,
	mosi: Peri<'static, PIN_7>,
	dma: Peri<'static, DMA_CH0>
) {

	//lcd backend start
	let lcd_backend = Box::new(LcdBackend::default());
	let lcd_keyboard_window = lcd_backend.keyboard_window.clone();
	let lcd_window = lcd_backend.window.clone();
	lcd_keyboard_window.set_size(slint::PhysicalSize::new(128, 41));
	lcd_keyboard_window.set_minimized(true);
	lcd_window.set_size(slint::PhysicalSize::new(128, 64));

	slint::platform::set_platform(lcd_backend).unwrap();

	// pio encoder
	let prg = PioEncoderProgram::new(pio);
	let pio_encoder: PioEncoder<'static, PIO0, 0> = PioEncoder::new(pio, pio_sm, enc_a, enc_b, &prg);

	//spawn lcd task
	spawner.spawn(lcd_task2(
		spi,
		sclk,
		mosi,
		dma,
		lcd_window.clone(),
		lcd_keyboard_window.clone()
	).unwrap());

	spawner.spawn(encoder_input_task(
		lcd_window.clone(),
		lcd_keyboard_window.clone(),
		pio_encoder
	).unwrap());
	spawner.spawn(encoder_button_input_task(
		lcd_window.clone(),
		lcd_keyboard_window.clone(),
		enc_button
	).unwrap());

	let ui2 = KeyboardWindow::new().unwrap();
	ui2.show().unwrap();

	//start ui
	let ui = EmbeddedUI::new().unwrap();
	ui.show().unwrap();

	keyboard(&ui, &ui2);

	spawner.spawn(initial_configuration_ui_task(ui.clone_strong(), config).unwrap());
}