use embassy_time::{Duration, Timer};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use alloc::rc::Rc;
use serde::Serialize;
use heapless::Deque;
use crate::sensor_manager::SharedSensorData;


#[derive(Clone, Copy, Debug, Serialize, Default)]
pub struct HistoryEntry {
    pub ts: u64, // Uptime in seconds (or minutes?) User asked for 10 mins.
    pub temp: f32,
    pub hum: u8,
    pub soil: f32,
    pub ec: f32,
}

pub type SharedHistory = Rc<Mutex<CriticalSectionRawMutex, Deque<HistoryEntry, 10>>>;

#[embassy_executor::task]
pub async fn history_task(
    shared_history: SharedHistory,
    shared_sensor: SharedSensorData,
) {
    loop {
        // Run every 1 minute
        Timer::after(Duration::from_secs(60)).await;
        
        // Capture Sensor Data
        let (temp, hum, soil, ec) = {
            let s = shared_sensor.lock().await;
            let t = s.internal.map(|r| r.temp.to_num::<f32>()).unwrap_or(0.0);
            let h = s.internal.map(|r| r.hum).unwrap_or(0);
            let soil_v = s.soil_moisture.map(|v| v.to_num::<f32>()).unwrap_or(0.0);
            let ec_v = s.ec_level.map(|v| v.to_num::<f32>()).unwrap_or(0.0);
            (t, h, soil_v, ec_v)
        };
        
        let entry = HistoryEntry {
            ts: embassy_time::Instant::now().as_secs(),
            temp,
            hum,
            soil,
            ec
        };
        
        // Push to History
        {
            let mut hist = shared_history.lock().await;
            if hist.is_full() {
                hist.pop_front();
            }
            let _ = hist.push_back(entry);
            // defmt::info!("History: Added entry ts={}", entry.ts);
        }
    }
}
