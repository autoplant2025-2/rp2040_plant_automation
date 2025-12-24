use serde::{Serialize, Deserialize};
use heapless::Vec;
use heapless::String;




// Re-export or redefine ControlConfig if needed. 
// For now, we assume ControlConfig will be updated to derive Serialize/Deserialize in control.rs
// But to avoid circular deps or complex refactoring, let's define the persistence wrappers here.
// Actually, it's better to have ControlConfig derive Serialize/Deserialize in control.rs and use it here.
// So we will import it.

use crate::control::ControlConfig;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CalibrationData {
    pub pid_config: ControlConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DeviceSettings {
    pub wifi_ssid: String<32>,
    pub wifi_password: Option<String<64>>,
    pub timezone_offset: i32, // Seconds
    pub last_datetime: u64, // Unix timestamp
}

impl Default for DeviceSettings {
    fn default() -> Self {
        Self {
            wifi_ssid: String::new(),
            wifi_password: None,
            timezone_offset: 0,
            last_datetime: 0,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PlantConfiguration {
    pub plant_name: String<32>,
    pub start_timestamp: Option<u64>,
    pub nominal_ec: f32,
    pub script_source: Vec<u8, 2048>,
    pub target_temp: f32,
    pub light_start_hour: u8,
    pub light_end_hour: u8,
    pub light_intensity: u8,
}

impl Default for PlantConfiguration {
    fn default() -> Self {
        let mut name = String::new();
        name.push_str("My Plant").ok();
        Self {
            plant_name: name,
            start_timestamp: None,
            nominal_ec: 1.2,
            script_source: Vec::new(),
            target_temp: 25.0,
            light_start_hour: 8,
            light_end_hour: 20,
            light_intensity: 255,
        }
    }
}
