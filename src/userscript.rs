use crate::control::TargetState;
use crate::sensor_manager::SensorData;
use fixed::types::I16F16;
use alloc::string::{String, ToString};

type Number = I16F16;

pub struct UserScript {
    source: String,
}

impl UserScript {
    pub fn new() -> Self {
        defmt::info!("Blisp Script Engine Init");
        Self {
            source: String::new(),
        }
    }

    pub fn update_script(&mut self, script: &str) -> Result<(), String> {
        self.source = script.to_string();
        Ok(())
    }


    pub fn calculate_targets(&mut self, sensors: &SensorData, _current_time: u64, days_since_start: u32, _time_str: &str) -> TargetState {
        let mut full_script = String::new();
        
        // Wrap script in a let block to define variables
        // (let ((temp 25.0) (humidity 50.0) ...) script)
        
        full_script.push_str("(let (");
        
        let temp_f = sensors.internal.map(|r| r.temp.to_num::<f32>()).unwrap_or(25.0);
        full_script.push_str(&alloc::format!("(temp {}) ", temp_f));
        
        let hum_f = sensors.internal.map(|r| r.hum).unwrap_or(50) as f32;
        full_script.push_str(&alloc::format!("(humidity {}) ", hum_f));

        let soil_f = sensors.soil_moisture.map(|v| v.to_num::<f32>()).unwrap_or(0.0);
        full_script.push_str(&alloc::format!("(soil {}) ", soil_f));

        let ec_f = sensors.ec_level.map(|v| v.to_num::<f32>()).unwrap_or(0.0);
        full_script.push_str(&alloc::format!("(ec {}) ", ec_f));

        let co2_f = sensors.co2_level.map(|v| v.to_num::<f32>()).unwrap_or(0.0);
        full_script.push_str(&alloc::format!("(co2 {}) ", co2_f));

        full_script.push_str(&alloc::format!("(days {}) ", days_since_start));
        // Strings might cause parse issues if blisp doesn't support them fully yet
        // full_script.push_str(&alloc::format!("(time \"{}\") ", time_str));
        
        full_script.push_str(") \n"); // Close definitions list and add newline
        
        full_script.push_str(&self.source);
        full_script.push_str(")"); // Close let
        
        // defmt::info!("Full Script:\n{}", full_script.as_str());

        // Context creation
        // Flow: init -> typing -> eval
        
        match blisp::init(&full_script) {
            Ok(exprs) => {
                // blisp::typing is a function
                match blisp::typing(&exprs) {
                    Ok(ctx) => {
                        // defmt::info!("Context created");
                        match blisp::eval(&full_script, &ctx) {
                             Ok(expr) => {
                                 defmt::info!("Script Success: {:?}", alloc::format!("{:?}", expr).as_str());
                                 // TODO: extract values from expr
                             }
                             Err(e) => {
                                 defmt::error!("Script Eval Failed: {:?}", alloc::format!("{:?}", e).as_str());
                             }
                        }
                    }
                    Err(e) => {
                        defmt::error!("Script Typing Failed: {:?}", alloc::format!("{:?}", e).as_str());
                    }
                }
            }
            Err(e) => {
                defmt::error!("Script Init Failed: {:?}", alloc::format!("{:?}", e).as_str());
            }
        }

        // defmt::info!("Script eval stubbed: Context creation API missing");
        
        TargetState {
            temp: Number::from_num(25.0),
            humidity: 60,
            vent_on: true,
            light_intensity: 255,
        }
    }
}
