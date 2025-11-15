use alloc::borrow::ToOwned;
use alloc::rc::Rc;
use alloc::string::String;
use core::cell::RefCell;
use core::sync::atomic::Ordering;
use embassy_time::Instant;
use portable_atomic::AtomicU8;
use slint::platform::{Clipboard, Platform, WindowAdapter};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::PlatformError;

// ui의 플랫폼 백엔드(lcd랑은 관련없음) ui 시간 구현
// 첫번째로 생성된 윈도우가 메인 두번째가 키보드
pub struct LcdBackend {
	pub window_count: AtomicU8,
	pub window: Rc<MinimalSoftwareWindow>,
	pub keyboard_window: Rc<MinimalSoftwareWindow>,
	pub clipboard: RefCell<Option<String>>
}

impl Default for LcdBackend {
	fn default() -> Self {
		Self {
			window_count: AtomicU8::new(0),
			window: MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer),
			keyboard_window: MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer),
			clipboard: Default::default(),
		}
	}
}

impl Platform for LcdBackend {
	fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
		let fetch = self.window_count.fetch_add(1, Ordering::Relaxed);
		if fetch == 0 {
			Ok(self.window.clone())
		} else if fetch == 1 {
			Ok(self.keyboard_window.clone())
		} else {
			self.window_count.store(2, Ordering::Relaxed);
			Err(PlatformError::NoPlatform)
		}
	}

	fn duration_since_start(&self) -> core::time::Duration {
		Instant::now().duration_since(Instant::from_secs(0)).into()
	}

	fn set_clipboard_text(&self, text: &str, _clipboard: Clipboard) {
		self.clipboard.replace(Some(text.to_owned()));
	}

	fn clipboard_text(&self, _clipboard: Clipboard) -> Option<String> {
		(*self.clipboard.borrow()).clone()
	}
}

