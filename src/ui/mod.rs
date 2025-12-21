mod encoder_button_input_task;
mod encoder_input_task;
mod keyboard;
mod lcd_backend;
mod lcd_task;
mod initial_configuration;
mod dashboard_task;

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
use crate::time_manager::SharedTimeManager;
use crate::config_manager::SharedConfig;
use crate::network::ShareNetworkStack;
use crate::network::wifi::SharedWifiControl;
use crate::ui::initial_configuration::initial_configuration_ui_task;
use crate::ui::dashboard_task::dashboard_task;
use crate::sensor_manager::SharedSensorData;
use crate::hardware_manager::SharedActuatorState;

use slint::SharedString;
slint::include_modules!();

pub fn init_ui(
	spawner: &Spawner,
	config: SharedConfig,
	wifi_control: SharedWifiControl,
	network_stack: ShareNetworkStack,
    time_manager: SharedTimeManager,
    sensor_data: SharedSensorData,
    actuator_state: SharedActuatorState,
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

	let status_global = ui.global::<Status>();
	let config_clone_for_status = config.clone();
	let net_stack_for_status = network_stack.clone();
	status_global.on_wifi_status(move || {
		let is_connected = if let Ok(stack) = net_stack_for_status.try_lock() {
			stack.is_link_up()
		} else {
			false
		};

		let mut ssid_str = SharedString::default();
		if is_connected {
			if let Ok(cfg) = config_clone_for_status.try_lock() {
				let settings = cfg.settings();
				if !settings.wifi_ssid.is_empty() {
					ssid_str = SharedString::from(settings.wifi_ssid.as_str());
				}
			}
		}
		
		WifiStatus {
			connected: is_connected,
			ssid: ssid_str,
		}
	});

	let time_manager_status = time_manager.clone();
    let config_clone_for_time = config.clone();
	status_global.on_current_time(move || {
		if let Some(utc) = time_manager_status.get_time() {
            let offset_sec = if let Ok(cfg) = config_clone_for_time.try_lock() {
                cfg.settings().timezone_offset
            } else {
                9 * 3600 // Default fallback
            };

			if let Some(offset) = chrono::FixedOffset::east_opt(offset_sec) {
				let local = utc.with_timezone(&offset);
				return slint::SharedString::from(alloc::format!("{}", local.format("%H:%M:%S")).as_str());
			}
		}
		slint::SharedString::from("Ready..")
	});

	let time_manager_date = time_manager.clone();
    let config_clone_for_date = config.clone();
	status_global.on_current_date(move || {
		if let Some(utc) = time_manager_date.get_time() {
            let offset_sec = if let Ok(cfg) = config_clone_for_date.try_lock() {
                cfg.settings().timezone_offset
            } else {
                9 * 3600 // Default fallback
            };

			if let Some(offset) = chrono::FixedOffset::east_opt(offset_sec) {
				let local = utc.with_timezone(&offset);
				return slint::SharedString::from(alloc::format!("{}", local.format("%Y-%m-%d")).as_str());
			}
		}
		slint::SharedString::from("Ready..")
	});

    let net_stack_for_ip = network_stack.clone();
	status_global.on_ip_address(move || {
		if let Ok(stack) = net_stack_for_ip.try_lock() {
			if let Some(config) = stack.config_v4() {
                return slint::SharedString::from(alloc::format!("http://{}", config.address).as_str());
            }
		}
        slint::SharedString::from("No Network")
	});

    status_global.on_generate_qr_code(move |text| {
        use qrcode2::QrCode;
        use slint::{SharedPixelBuffer, Rgb8Pixel, Image};
        
        if let Ok(qr) = QrCode::new(text.as_str()) {
             let size = qr.width() as u32;
             let mut pixel_buffer = SharedPixelBuffer::<Rgb8Pixel>::new(size, size);
             let buf = pixel_buffer.make_mut_bytes();
             
             for y in 0..size {
                 for x in 0..size {
                     // qrcode2: QrCode implements Index<(usize, usize)> returning Color
                     let color_enum = qr[(x as usize, y as usize)];
                     let is_dark = match color_enum {
                         qrcode2::Color::Dark => true,
                         _ => false,
                     };
                     
                     let color = if is_dark { // lcd color is inverted
                         [255, 255, 255] // Black
                     } else {
                         [0, 0, 0] // White
                     };
                     let offset = ((y * size + x) * 3) as usize;
                     buf[offset] = color[0];
                     buf[offset+1] = color[1];
                     buf[offset+2] = color[2];
                 }
             }
             return Image::from_rgb8(pixel_buffer);
        }
        
        Image::default()
    });

	spawner.spawn(initial_configuration_ui_task(ui.clone_strong(), config.clone(), wifi_control, network_stack, time_manager).unwrap());
    // Pass strong reference to keep UI alive
    spawner.spawn(dashboard_task(ui.clone_strong(), config, sensor_data, actuator_state).unwrap());
}