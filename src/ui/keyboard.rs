use slint::{ComponentHandle, SharedString};
use slint::platform::WindowEvent;
use crate::ui::{EmbeddedUI, KeyboardWindow, KeyboardWindowLogic, VirtualKeyboardHandler};

pub fn keyboard(component_handle: &EmbeddedUI, keyboard: &KeyboardWindow)
{
	let weak_keyboard_window = keyboard.as_weak();
	let weak_main_component_handle = component_handle.as_weak();
	keyboard.on_close(move || {
		if let Some(keyboard_window) = weak_keyboard_window.upgrade() {
			keyboard_window.window().set_minimized(true);
			keyboard_window.window().request_redraw();
		}
		if let Some(component_handle2) = weak_main_component_handle.upgrade() {
			let main_window = component_handle2.window();
			let mut size = main_window.size();
			size.height = 64;
			main_window.set_size(size);
			let global = component_handle2.global::<KeyboardWindowLogic>();
			global.set_keyboard_open(false);
		}
	});
	let weak_keyboard_window = keyboard.as_weak();
	let weak_main_component_handle = component_handle.as_weak();
	component_handle.global::<KeyboardWindowLogic>().on_open(move || {
		if let Some(keyboard_window) = weak_keyboard_window.upgrade() {
			keyboard_window.window().set_minimized(false);
			keyboard_window.window().request_redraw();
		}
		if let Some(component_handle2) = weak_main_component_handle.upgrade() {
			let main_window = component_handle2.window();
			let mut size = main_window.size();
			size.height = 23;
			main_window.set_size(size);
			let global = component_handle2.global::<KeyboardWindowLogic>();
			global.set_keyboard_open(true);
		}
	});
	let weak_main_component_handle = component_handle.as_weak();
	keyboard.global::<VirtualKeyboardHandler>().on_key_down(move |key: SharedString| {
		if let Some(component_handle2) = weak_main_component_handle.upgrade() {
			component_handle2.window().dispatch_event(WindowEvent::KeyPressed {text: key});
		}
	});
	let weak_main_component_handle = component_handle.as_weak();
	keyboard.global::<VirtualKeyboardHandler>().on_key_down_repeat(move |key: SharedString| {
		if let Some(component_handle2) = weak_main_component_handle.upgrade() {
			component_handle2.window().dispatch_event(WindowEvent::KeyPressRepeated {text: key});
		}
	});
	let weak_main_component_handle = component_handle.as_weak();
	keyboard.global::<VirtualKeyboardHandler>().on_key_up(move |key: SharedString| {
		if let Some(component_handle2) = weak_main_component_handle.upgrade() {
			component_handle2.window().dispatch_event(WindowEvent::KeyReleased {text: key});
		}
	});
}