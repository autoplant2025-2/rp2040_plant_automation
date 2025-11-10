use alloc::rc::Rc;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::Peri;
use embassy_rp::peripherals::{PIN_2, PIO0};
use embassy_rp::pio_programs::rotary_encoder::{Direction, PioEncoder};
use embassy_time::Timer;
use slint::platform::software_renderer::MinimalSoftwareWindow;
use slint::platform::{Key, WindowEvent};
use slint::SharedString;

//사용자 입력 (로터리 인코더, 클릭) 을 처리하는 태스크
#[embassy_executor::task]
pub async fn encoder_button_task(
	window: Rc<MinimalSoftwareWindow>,
	mut enc: PioEncoder<'static, PIO0, 0>,
	click: Peri<'static, PIN_2>
) {
	//let mut encoder = RotaryEncoder::<_, _, _, Infallible>::new(Input::new(en1, Pull::Up), Input::new(en2, Pull::Up), DDDD).unwrap();
	// let mut i1 = Input::new(en1, Pull::Up);
	// let mut i2 = Input::new(en2, Pull::Up);
	let mut click = Input::new(click, Pull::Up);
	let tab = SharedString::from(Key::Tab);
	let backtab = SharedString::from(Key::Backtab);
	let enter = SharedString::from(Key::Return);
	loop {
		match select(
			enc.read(),
			click.wait_for_rising_edge()
		).await {
			Either::First(en) => window.dispatch_event(WindowEvent::KeyPressed { text: match en {
				Direction::Clockwise => tab.clone(),
				Direction::CounterClockwise => backtab.clone()
			}}),
			Either::Second(_) => {
				window.dispatch_event(WindowEvent::KeyPressed { text: enter.clone() });
				Timer::after_millis(100).await;
			}
		};
	}
}