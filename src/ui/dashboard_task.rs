use embassy_time::{Timer, Duration};
use crate::config_manager::SharedConfig;
use crate::sensor_manager::SharedSensorData;
use crate::hardware_manager::SharedActuatorState;
use slint::ComponentHandle;
// Import the generated slint module. The parent module `ui` has `slint::include_modules!()`.
// We need to import the globals from that.
// It seems `ui/mod.rs` includes modules. They are available in `crate::ui`.
use crate::ui::{EmbeddedUI, PlantValue, Plant, PlantNameValue, PlantName};

#[embassy_executor::task]
pub async fn dashboard_task(
    ui: EmbeddedUI,
    config: SharedConfig,
    sensor_data: SharedSensorData,
    actuator_state: SharedActuatorState,
) {
    loop {
        // ... (lines 19-46 unchanged)
        Timer::after(Duration::from_millis(500)).await;

        let sensors = {
            let data = sensor_data.lock().await;
            data.clone()
        };

        let actuators = {
            let state = actuator_state.lock().await;
            *state
        };

        // defmt::info!("Dash Task: Actuators: {}", actuators);

        let (target_temp, target_hum, plant_name, start_ts) = {
            let cfg = config.lock().await;
            let pc = cfg.plant_config();
            (pc.target_temp, 60, pc.plant_name.clone(), pc.start_timestamp) 
        };

        let day = if let Some(_start) = start_ts {
             0 
        } else {
            0
        };

        let current_temp = sensors.internal.map(|r| r.temp.to_num::<i32>()).unwrap_or(0);
        let current_hum = sensors.internal.map(|r| r.hum as i32).unwrap_or(0);
        let out_temp = sensors.external.map(|r| r.temp.to_num::<i32>()).unwrap_or(0);
        let out_hum = sensors.external.map(|r| r.hum as i32).unwrap_or(0);

        let fan_val = actuators.fan_inner_speed as i32 * 100 / 255;
        let light_val = actuators.led_intensity as i32 * 100 / 255;
        let water_val = if actuators.pump_nutrient { 100 } else { 0 }; 
        
        let cool_val = if !actuators.peltier_temp_dir && actuators.peltier_temp_pwm > 0 {
            actuators.peltier_temp_pwm as i32 * 100 / 255
        } else {
            0
        };

        let p = Plant {
            Fan: fan_val,
            Light: light_val,
            Water: water_val,
            Hum: current_hum,
            HumTarget: target_hum,
            Temp: current_temp,
            TempTarget: target_temp as i32,
            Cool: cool_val,
            OutTemp: out_temp,
            OutHum: out_hum,
        };

        let name_str = slint::SharedString::from(plant_name.as_str());
        let pn = PlantName {
            name: name_str,
            day: day,
        };

        let p_global = ui.global::<PlantValue>();
        let n_global = ui.global::<PlantNameValue>();
        
        p_global.set_plant(p);
        n_global.set_plantname(pn);
    }
}
