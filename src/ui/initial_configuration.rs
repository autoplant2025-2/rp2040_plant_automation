use alloc::rc::Rc;
use alloc::vec::Vec;
use chrono::TimeZone;
use cyw43::{Control, JoinOptions, Scanner};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embassy_time::Timer;
use slint::{ComponentHandle, ModelRc, VecModel};
use crate::config_manager::SharedConfig;
use crate::network::{wifi, ShareNetworkStack};
use crate::network::wifi::{SharedWifiControl, WifiSecurity};
use crate::ui::{DateTime, EmbeddedUI, InitUILogic, WifiNetwork};
use crate::time_manager::SharedTimeManager;

#[embassy_executor::task]
pub async fn initial_configuration_ui_task(
	ui: EmbeddedUI,
	config: SharedConfig,
	wifi_control: SharedWifiControl,
	network_stack: ShareNetworkStack,
    time_manager: SharedTimeManager,
) -> ! {
	enum InitAction {
		SetTime(DateTime),
		WifiConnect(alloc::string::String, Option<alloc::string::String>),
		ScanWifi,
        SetTimezone(i32),
	}
	let init_logic = ui.global::<InitUILogic>();
	// Set default time directly on the global property
	let mut default_val = DateTime {
		year: 2025, month: 1, day: 1,
		hour: 12, minute: 0, second: 0, tz: 9
	};

	// Try to load from persistence
	{
        let cfg = config.lock().await;
        let settings = cfg.settings();
        if settings.last_datetime > 0 {
            if let Some(utc) = chrono::DateTime::from_timestamp(settings.last_datetime as i64, 0) {
                let offset_secs = settings.timezone_offset;
                if let Some(offset) = chrono::FixedOffset::east_opt(offset_secs) {
                    let local = utc.with_timezone(&offset);
                    use chrono::{Datelike, Timelike};
                    default_val.year = local.year();
                    default_val.month = local.month() as i32;
                    default_val.day = local.day() as i32;
                    default_val.hour = local.hour() as i32;
                    default_val.minute = local.minute() as i32;
                    default_val.second = local.second() as i32;
                    default_val.tz = offset_secs / 3600;
                }
            }
        }
	}

	init_logic.set_default_time(default_val);

	let signal = Rc::new(Signal::<CriticalSectionRawMutex, InitAction>::new());
	let signal_cb_time = signal.clone();
	let signal_cb_wifi = signal.clone();
	let signal_cb_scan = signal.clone();
    let signal_cb_tz = signal.clone();

	//setup callback
	init_logic.on_set_time(move |dt| {
		signal_cb_time.signal(InitAction::SetTime(dt));
	});

	// Callback for WiFi
	init_logic.on_connect_wifi(move |ssid, has_pass, pass| {
		let p = if has_pass {
			Some(pass.into())
		} else {
			None
		};
		signal_cb_wifi.signal(InitAction::WifiConnect(ssid.into(), p));
	});

	// Callback for Scan
	init_logic.on_scan_wifi(move || {
		signal_cb_scan.signal(InitAction::ScanWifi);
	});

    init_logic.on_set_timezone(move |tz| {
        signal_cb_tz.signal(InitAction::SetTimezone(tz));
    });



	{
		let cfg = config.lock().await;
		let settings = cfg.settings();
		if !settings.wifi_ssid.is_empty() {
			init_logic.set_state(0); // Connecting (Splash)
			init_logic.set_selected_ssid(settings.wifi_ssid.as_str().into());
			if let Some(pwd) = settings.wifi_password.as_ref() {
				init_logic.set_password(pwd.as_str().into());
			}
			signal.signal(InitAction::WifiConnect(
				settings.wifi_ssid.as_str().into(),
				settings.wifi_password.as_ref().map(|x| x.as_str().into())
			));
		} else {
			init_logic.set_state(2); // WiFi Setup Screen
			signal.signal(InitAction::ScanWifi);
		}
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

				// Update TimeManager
				// dt is in Local Time. We need to convert it to UTC.
				if let Some(naive_date) = chrono::NaiveDate::from_ymd_opt(dt.year, dt.month as u32, dt.day as u32) {
					if let Some(naive_dt) = naive_date.and_hms_opt(dt.hour as u32, dt.minute as u32, dt.second as u32) {
						let offset_secs = dt.tz * 3600;
						if let Some(offset) = chrono::FixedOffset::east_opt(offset_secs) {
							// Treat naive_dt as local time in that offset
							if let chrono::LocalResult::Single(local_dt) = offset.from_local_datetime(&naive_dt) {
								let utc_dt = local_dt.with_timezone(&chrono::Utc);
								time_manager.set_time(utc_dt);
								defmt::info!("TimeManager updated manually to UTC: {}", utc_dt.timestamp());
							}
						}
					}
				}

				init_logic.set_state(4 + 5); // Go to Timezone Select
			}
			InitAction::WifiConnect(ssid, pass) => {
				init_logic.set_status_message("와이파이 연결중...".into());
				defmt::info!("User requested WiFi connect: S:{} P:{}", ssid, pass);

                // Disable auto-reconnect while manually connecting
                crate::network::WIFI_AUTOCONNECT_ENABLED.store(false, portable_atomic::Ordering::Relaxed);

				{
					let mut cfg = config.lock().await;
					cfg.update_settings(|s| {
						if let Ok(ssid) = heapless::String::try_from(ssid.as_str()) {
							s.wifi_ssid = ssid;
						} else {
							defmt::error!("wifi ssid with illegal length, not saving");
						}
						s.wifi_password = pass.as_ref().and_then(|pass| {
							if let Ok(pass) = heapless::String::try_from(pass.as_str()) {
								Some(pass)
							} else {
								defmt::error!("wifi pass with illegal length, not saving");
								None
							}
						});
					}).await;
				}
				let options = pass.as_ref().map(|pass| JoinOptions::new(pass.as_bytes())).unwrap_or(JoinOptions::new_open());
				let mut wifi_control = wifi_control.lock().await;
				let network_stack = network_stack.lock().await;
				let mut connected = false;
                for i in 1..=3 {
                     init_logic.set_status_message(slint::SharedString::from(alloc::format!("와이파이 연결중... ({}/3)", i).as_str()));
                     
                     // Ensure clean state before connecting
                     if let Err(e) = wifi_control.leave().await {
                        defmt::debug!("wifi leave error: {:?}", e);
                     }

                     if let Err(e) = wifi_control.join(ssid.as_str(), options.clone()).await {
                        defmt::info!("WiFi network join error (attempt {}): {:?}", i, e);
                         if i < 3 {
                             Timer::after(embassy_time::Duration::from_secs(3)).await;
                         }
                    } else {
                        connected = true;
                        break;
                    }
                }

                if !connected {
					init_logic.set_status_message("와이파이 연결 실패".into());
                    // Re-enable auto-reconnect on failure
                    crate::network::WIFI_AUTOCONNECT_ENABLED.store(true, portable_atomic::Ordering::Relaxed);
					Timer::after(embassy_time::Duration::from_secs(1)).await;
					init_logic.set_state(2 + 5);
					signal.signal(InitAction::ScanWifi);
					continue;
                }

                // Re-enable auto-reconnect on success
                crate::network::WIFI_AUTOCONNECT_ENABLED.store(true, portable_atomic::Ordering::Relaxed);

				network_stack.wait_link_up().await;
				network_stack.wait_config_up().await;

				init_logic.set_status_message("시간 동기화 중...".into());

                let mut success = false;
                for i in 1..=3 {
                    init_logic.set_status_message(slint::SharedString::from(alloc::format!("시간 동기화 중... ({}/3)", i).as_str()));
                    if let Ok(()) = time_manager.sync_time((*network_stack).clone(), 1).await {
                        if let Some(time) = time_manager.get_time() {
                            let mut cfg = config.lock().await;
                            cfg.update_settings(|s| {
                            s.last_datetime = time.timestamp() as u64;
                            }).await;
                            defmt::info!("Initial time sync successful and saved.");
                            init_logic.set_state(4 + 5); // Go to Timezone Select
                            success = true;
                            break;
                        }
                    } else {
                        defmt::warn!("Initial time sync failed attempt {}", i);
                    }
                }

                if success {
                    continue;
                }

				init_logic.set_status_message("시간 동기화 실패".into());
				Timer::after(embassy_time::Duration::from_secs(1)).await;
				init_logic.set_state(2 + 5);
				signal.signal(InitAction::ScanWifi);

			}
			InitAction::ScanWifi => {
				defmt::info!("Scanning WiFi...");
				init_logic.set_scanning(true);
				let model = Rc::new(VecModel::from(search_network(&mut *wifi_control.lock().await).await));
				init_logic.set_available_networks(ModelRc::from(model));
				init_logic.set_scanning(false);
			}
            InitAction::SetTimezone(tz) => {
                let mut cfg = config.lock().await;
				cfg.update_settings(|s| {
					s.timezone_offset = tz * 3600;
					defmt::info!("Timezone set to UTC{}", tz);
				}).await;
                init_logic.set_init_complete(true);
            }
		}
	}
}

async fn search_network(wifi_control: &mut Control<'_>) -> Vec<WifiNetwork> {
	let mut networks = Vec::with_capacity(16);
	let mut scanner: Scanner = match wifi_control.scan(Default::default()).await {
        Ok(s) => s,
        Err(e) => {
            defmt::error!("Scan failed status={}", e);
            return networks;
        }
    };
	while networks.len() < 16 {
		if let Some(bss_info) = scanner.next().await {
			let security = wifi::get_wifi_security(&bss_info);
			if security == WifiSecurity::Other || bss_info.ssid_len == 0 {
				continue;
			}
			let ssid_bytes = &bss_info.ssid[..bss_info.ssid_len as usize];
			let raw_ssid = match str::from_utf8(ssid_bytes) {
				Ok(s) => s,
				Err(_) => continue,
			};
			let clean_ssid = raw_ssid.trim_end_matches('\0');
			if clean_ssid.is_empty()
				|| clean_ssid.chars().any(|c| c.is_control())
				|| clean_ssid.trim().is_empty()
			{
				continue;
			}
			networks.push(WifiNetwork {
				ssid: clean_ssid.into(),
				rssi: bss_info.rssi as _,
				security: security == WifiSecurity::Password,
			})
		} else {
			break;
		}
	}
	networks
}