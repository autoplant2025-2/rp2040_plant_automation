use alloc::rc::Rc;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use slint::{ComponentHandle, ModelRc, VecModel, Weak};
use crate::config_manager::SharedConfig;
use crate::ui::{DateTime, EmbeddedUI, InitUILogic, WifiNetwork};

#[embassy_executor::task]
pub async fn initial_configuration_ui_task(
	ui: EmbeddedUI,
	config: SharedConfig
) -> ! {
	enum InitAction {
		SetTime(DateTime),
		WifiConnect(alloc::string::String, alloc::string::String),
		ScanWifi,
	}
	let init_logic = ui.global::<InitUILogic>();
	// Set default time directly on the global property
	let mut default_val = DateTime {
		year: 2025, month: 1, day: 1,
		hour: 12, minute: 0, second: 0, tz: 9
	};

	// Try to load from persistence
	{
		if let Ok(cfg) = config.try_lock() {
			let settings = cfg.settings();
			if settings.last_datetime > 0 {
				// Conversion logic (mocked for now)
			}
		}
	}

	init_logic.set_default_time(default_val);

	let signal = Rc::new(Signal::<CriticalSectionRawMutex, InitAction>::new());
	let signal_cb_time = signal.clone();
	let signal_cb_wifi = signal.clone();
	let signal_cb_scan = signal.clone();

	//setup callback
	init_logic.on_set_time(move |dt| {
		signal_cb_time.signal(InitAction::SetTime(dt));
	});

	// Callback for WiFi
	init_logic.on_connect_wifi(move |ssid, pass| {
		signal_cb_wifi.signal(InitAction::WifiConnect(ssid.into(), pass.into()));
	});

	// Callback for Scan
	init_logic.on_scan_wifi(move || {
		signal_cb_scan.signal(InitAction::ScanWifi);
	});

	if true /* wifi credential exist */ {
		init_logic.set_state(0); // Connecting (Splash)
		signal.signal(InitAction::WifiConnect("".into(), "".into()));
	} else {
		init_logic.set_state(3); // WiFi Setup Screen
		signal.signal(InitAction::ScanWifi);
	}

	loop {
		// Wait for user input
		let action = signal.wait().await;

		match action {
			InitAction::SetTime(dt) => {
				// Save time and finish
				let mut cfg = config.lock().await;
				cfg.update_settings(|s| {
					s.timezone_offset = dt.tz;
					defmt::info!("Manual time set: {}-{}-{} {}:{}", dt.year, dt.month, dt.day, dt.hour, dt.minute);
				}).await;
				init_logic.set_init_complete(true);
			}
			InitAction::WifiConnect(ssid, pass) => {
				defmt::error!("User requested WiFi connect: S:{} P:{}", ssid, pass);
				// Save credentials to config
				// {
				// 	let mut cfg = config.lock().await;
				// 	cfg.update_settings(|s| {
				// 		s.wifi_ssid.clear();
				// 		s.wifi_ssid.push_str(&ssid).ok();
				// 		s.wifi_password.clear();
				// 		s.wifi_password.push_str(&pass).ok();
				// 	}).await;
				// }
				Timer::after_secs(2).await; // Mock duration
				// if success, init_logic.set_init_complete(true);
				// if fail, init_logic.set_state(3); // WiFi Setup Screen
				init_logic.set_state(3);
				signal.signal(InitAction::ScanWifi);
			}
			InitAction::ScanWifi => {
				defmt::info!("Scanning WiFi...");
				init_logic.set_scanning(true);

				Timer::after_secs(2).await; // Mock duration

				let mut networks = alloc::vec![];
				networks.push(WifiNetwork { ssid: "HomeWiFi".into(), rssi: -50, security: "WPA2".into() });
				networks.push(WifiNetwork { ssid: "Guest".into(), rssi: -70, security: "OPEN".into() });
				networks.push(WifiNetwork { ssid: "Office".into(), rssi: -60, security: "WPA2".into() });
				networks.push(WifiNetwork { ssid: "Office".into(), rssi: -60, security: "WPA2".into() });
				networks.push(WifiNetwork { ssid: "Office".into(), rssi: -60, security: "WPA2".into() });

				let model = Rc::new(VecModel::from(networks));
				init_logic.set_available_networks(ModelRc::from(model));
				init_logic.set_scanning(false);
				// Continue waiting for signals (Scan done, user selects and connects or rescans)
			}
		}
	}
}