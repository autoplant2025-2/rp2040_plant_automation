use alloc::rc::Rc;
use embassy_time::Instant;
use slint::platform::{Platform, WindowAdapter};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::PlatformError;

// ui의 플랫폼 백엔드(lcd랑은 관련없음) ui 시간 구현
pub struct LcdBackend {
	pub window: Rc<MinimalSoftwareWindow>,
}

impl Default for LcdBackend {
	fn default() -> Self {
		Self {
			window: MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer)
		}
	}
}

impl Platform for LcdBackend {
	fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
		let window = self.window.clone();
		Ok(window)
	}

	fn duration_since_start(&self) -> core::time::Duration {
		Instant::now().duration_since(Instant::from_secs(0)).into()
	}
}