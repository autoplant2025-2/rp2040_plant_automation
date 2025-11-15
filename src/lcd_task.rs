use alloc::rc::Rc;
use dummy_pin::DummyPin;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_futures::yield_now;
use embassy_rp::{spi, Peri};
use embassy_rp::peripherals::{DMA_CH0, PIN_14, PIN_15, SPI1};
use embassy_rp::spi::{Phase, Polarity, Spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use rgb::Rgb;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{update_timers_and_animations, WindowAdapter};
use st7920_async::{SpiPixel8, SpiScreenBuffer, St7920SpiGdRam};
use crate::EmbassyDelay;

pub fn render(window: &MinimalSoftwareWindow, buffer: &mut [Rgb<u8>], force_update: bool) -> bool {
	window.draw_if_needed(|renderer| {
		if force_update {
			renderer.set_repaint_buffer_type(RepaintBufferType::NewBuffer);
		} else {
			renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
		}
		renderer.render(buffer, 128);
	})
}

// ui를 렌더링하고 lcd로 출력하는 태스크
#[embassy_executor::task]
pub async fn lcd_task2(
	spi: Peri<'static, SPI1>,
	gp14: Peri<'static, PIN_14>,
	gp15: Peri<'static, PIN_15>,
	dma: Peri<'static, DMA_CH0>,
	window: Rc<MinimalSoftwareWindow>,
	keyboard_window: Rc<MinimalSoftwareWindow>,
) {
	let mut config = spi::Config::default();
	config.frequency = 600_000;
	config.phase = Phase::CaptureOnSecondTransition;
	config.polarity = Polarity::IdleLow;
	let bus = Spi::new_txonly(
		spi,
		gp14,
		gp15,
		dma,
		config.clone()
	);
	let bus_mutex = Mutex::<NoopRawMutex, _>::new(bus);
	let device = SpiDeviceWithConfig::new(&bus_mutex, DummyPin::new_high(), config);

	let mut lcd = St7920SpiGdRam::new(device, EmbassyDelay).await.unwrap();
	let mut screen_buf = SpiScreenBuffer::<{ 256 / 16 }, 32>::default();


	// 30fps
	let mut ticker = Ticker::every(Duration::from_hz(30));
	let mut render_buffer = [Rgb::<u8>::default(); 128 * 64];

	let mut last_minimized = true;

	loop {
		ticker.next().await;
		update_timers_and_animations();
		yield_now().await;
		let current_minimized = keyboard_window.is_minimized();
		let force_update = current_minimized != last_minimized;
		last_minimized = current_minimized;
		let mut update = render(&*window, &mut render_buffer, force_update);
		if !keyboard_window.is_minimized() {
			update |= render(&*keyboard_window, &mut render_buffer[(128 * window.size().height as usize)..], force_update);
		}
		if update || force_update {
			yield_now().await;
			// top half of lcd x = 0..8, y = 0..32
			for y in 0..32 {
				for x in 0..8 {
					let pix16 = &mut screen_buf[y][x];
					let ref mut iter = (&render_buffer[128 * y + 16 * x..]).iter().map(|x| *x);
					pix16.0 = SpiPixel8::new(bitmap_nibble(iter), bitmap_nibble(iter));
					pix16.1 = SpiPixel8::new(bitmap_nibble(iter), bitmap_nibble(iter));
				}
			}
			yield_now().await;
			//bottom half of lcd x = 8..16, y = 0..32
			for y in 0..32 {
				for x in 8..16 {
					let pix16 = &mut screen_buf[y][x];
					let ref mut iter = (&render_buffer[128 * (y + 32) + 16 * (x - 8)..]).iter().map(|x| *x);
					pix16.0 = SpiPixel8::new(bitmap_nibble(iter), bitmap_nibble(iter));
					pix16.1 = SpiPixel8::new(bitmap_nibble(iter), bitmap_nibble(iter));
				}
			}
			lcd.write_screen(&mut screen_buf).await.unwrap();
		}
	}
}

fn bitmap_nibble(pixbuf: &mut impl Iterator<Item=Rgb<u8>>) -> u8 {
	let mut bitmap = 0;
	bitmap |= (luma(pixbuf.next().unwrap()) < 128) as u8;
	bitmap <<= 1;
	bitmap |= (luma(pixbuf.next().unwrap()) < 128) as u8;
	bitmap <<= 1;
	bitmap |= (luma(pixbuf.next().unwrap()) < 128) as u8;
	bitmap <<= 1;
	bitmap |= (luma(pixbuf.next().unwrap()) < 128) as u8;
	bitmap
}

fn luma(rgb: Rgb<u8>) -> u8 {
	let Rgb {r, g, b} = rgb;
	// Use integer weights approximating perceived brightness:
	// 0.299 * R + 0.587 * G + 0.114 * B ≈ (77 * R + 150 * G + 29 * B) >> 8
	let luminance = (77u16 * r as u16 + 150u16 * g as u16 + 29u16 * b as u16) >> 8;
	luminance as u8
}