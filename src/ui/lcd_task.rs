use alloc::rc::Rc;
use dummy_pin::DummyPin;
use embassy_embedded_hal::shared_bus::asynch::spi::SpiDeviceWithConfig;
use embassy_futures::yield_now;
use embassy_rp::{spi, Peri};
use embassy_rp::peripherals::{DMA_CH0, PIN_6, PIN_7, SPI0};
use embassy_rp::pio::Pio;
use embassy_rp::spi::{Phase, Polarity, Spi};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Ticker};
use rgb::Gray;
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, TargetPixel, PremultipliedRgbaColor};
use slint::platform::{update_timers_and_animations, WindowAdapter};
use st7920_async::{SpiPixel8, SpiScreenBuffer, St7920SpiGdRam};
use crate::EmbassyDelay;

#[derive(Copy, Clone, Debug, Default)]
pub struct GrayPixel(pub Gray<u8>);

impl TargetPixel for GrayPixel {
	fn blend(&mut self, color: PremultipliedRgbaColor) {
		let a = (255 - color.alpha) as u16;
		let r = color.red as u16;
		let g = color.green as u16;
		let b = color.blue as u16;
		let gray = (77 * r + 150 * g + 29 * b) >> 8;
		let v = self.0.value() as u16;
		self.0 = Gray((gray + (v * a) / 255) as u8);
	}

	fn from_rgb(r: u8, g: u8, b: u8) -> Self {
		let gray = (77u16 * r as u16 + 150u16 * g as u16 + 29u16 * b as u16) >> 8;
		GrayPixel(Gray(gray as u8))
	}
}

pub fn render(window: &MinimalSoftwareWindow, buffer: &mut [GrayPixel], force_update: bool) -> bool {
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

	spi: Peri<'static, SPI0>,
	gp6: Peri<'static, PIN_6>,
	gp7: Peri<'static, PIN_7>,
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
		gp6,
		gp7,
		dma,
		config.clone()
	);
	let bus_mutex = Mutex::<NoopRawMutex, _>::new(bus);
	let device = SpiDeviceWithConfig::new(&bus_mutex, DummyPin::new_high(), config);

	let mut lcd = St7920SpiGdRam::new(device, EmbassyDelay).await.unwrap();
	let mut screen_buf = SpiScreenBuffer::<{ 256 / 16 }, 32>::default();


	// 30fps
	let mut ticker = Ticker::every(Duration::from_hz(30));
	let mut render_buffer = [GrayPixel::default(); 128 * 64];

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

fn bitmap_nibble(pixbuf: &mut impl Iterator<Item=GrayPixel>) -> u8 {
	let mut bitmap = 0;
	bitmap |= (pixbuf.next().unwrap().0.value() < 128) as u8;
	bitmap <<= 1;
	bitmap |= (pixbuf.next().unwrap().0.value() < 128) as u8;
	bitmap <<= 1;
	bitmap |= (pixbuf.next().unwrap().0.value() < 128) as u8;
	bitmap <<= 1;
	bitmap |= (pixbuf.next().unwrap().0.value() < 128) as u8;
	bitmap
}