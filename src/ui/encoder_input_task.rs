use alloc::rc::Rc;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio_programs::rotary_encoder::{Direction, PioEncoder};
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{Key, WindowEvent};
use slint::SharedString;

//사용자 입력 (로터리 인코더, 클릭) 을 처리하는 태스크
#[embassy_executor::task]
pub async fn encoder_input_task(
	window: Rc<MinimalSoftwareWindow>,
	keyboard_window: Rc<MinimalSoftwareWindow>,
	mut enc: PioEncoder<'static, PIO0, 0>
) {
	let tab = SharedString::from(Key::Tab);
	let backtab = SharedString::from(Key::Backtab);

	loop {
		let en = enc.read().await;
		let w = if keyboard_window.is_minimized() {
			&window
		} else {
			&keyboard_window
		};
		w.dispatch_event(WindowEvent::KeyPressed { text: match en {
			Direction::CounterClockwise => tab.clone(),
			Direction::Clockwise => backtab.clone()
		}})
	}
}