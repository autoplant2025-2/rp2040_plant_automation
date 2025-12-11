#![allow(dead_code)]
#![allow(unused_variables)]


use fixed::types::I16F16;
use piddiy::PidController;
use serde::{Serialize, Deserialize};

// Type alias for our fixed-point number
pub type Number = I16F16;

/// PID Gains structure for cleaner configuration
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PidGains {
    pub kp: Number,
    pub ki: Number,
    pub kd: Number,
}

impl PidGains {
    pub fn new(kp: f32, ki: f32, kd: f32) -> Self {
        Self {
            kp: Number::from_num(kp),
            ki: Number::from_num(ki),
            kd: Number::from_num(kd),
        }
    }

    /// Apply these gains to a PidController
    pub fn apply_to(&self, pid: &mut PidController<Number, Number>) {
        pid.kp(self.kp).ki(self.ki).kd(self.kd);
    }
}

/// Configuration for the controller, including PID gains and limits.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ControlConfig {
    // Temperature Control (Cascade)
    pub air_temp: PidGains,      // Primary Loop (Air Temp -> Target Peltier Temp)
    pub peltier_temp_heat: PidGains, // Secondary Loop (Peltier Temp -> PWM)
    pub peltier_temp_cool: PidGains,

    // Humidity Loop
    pub hum_cold_side: PidGains,
    pub hum_cold_target: Number,

    // Feedforward Gains
    pub k_ff_hum: Number,
    pub k_ff_vent: Number,

    // Fan Control
    pub fan_temp_outer: PidGains,
    pub fan_hum_hot: PidGains,
    pub peltier_temp_diff_target: Number,

    pub k_fan_effort: Number,
    pub fan_base_day: Number,
    pub fan_base_night: Number,
    pub max_fan_speed: u8,

    // Soil Moisture Control
    pub soil_low_threshold: Number,
    pub soil_high_threshold: Number,
    
    // EC Control (Hysteresis)
    pub ec_low_threshold: Number,
    pub ec_high_threshold: Number,
}

impl Default for ControlConfig {
    fn default() -> Self {
        Self {
            air_temp: PidGains::new(1.0, 0.0, 0.0),
            peltier_temp_heat: PidGains::new(2.0, 0.1, 0.0),
            peltier_temp_cool: PidGains::new(5.0, 0.1, 0.0),
            hum_cold_side: PidGains::new(2.0, 0.1, 0.0),
            hum_cold_target: Number::from_num(5.0), // Target 5C for dehumidification
            k_ff_hum: Number::from_num(0.2), // Scaled down for 0-255 input
            k_ff_vent: Number::from_num(0.2),
            fan_temp_outer: PidGains::new(5.0, 0.1, 0.0),
            fan_hum_hot: PidGains::new(5.0, 0.1, 0.0),
            peltier_temp_diff_target: Number::from_num(10.0), // Target 10C difference (keep it low)
            k_fan_effort: Number::from_num(0.5),
            fan_base_day: Number::from_num(51), // ~20% of 255
            fan_base_night: Number::from_num(0),
            max_fan_speed: 204, // ~80% of 255
            soil_low_threshold: Number::from_num(100), // Wet (Stop)
            soil_high_threshold: Number::from_num(220), // Dry (Start)
            ec_low_threshold: Number::from_num(650.0), // ppm
            ec_high_threshold: Number::from_num(750.0), // ppm
        }
    }
}



// NTC Sensor Indices
pub const NTC_PELTIER_INNER: usize = 0;
pub const NTC_PELTIER_OUTER: usize = 1;
pub const NTC_PELTIER_HUM_COLD: usize = 2;
pub const NTC_PELTIER_HUM_HOT: usize = 3;



/// Target state determined by the script engine
#[derive(Clone, Debug, Default)]
pub struct TargetState {
    pub temp: Number,
    pub humidity: u8,
    pub vent_on: bool,
    pub light_intensity: u8,
}

mod adaptive_tuner;
use adaptive_tuner::AdaptiveTuner;

use crate::sensor_manager::SensorData;
use crate::hardware_manager::{ ActuatorOutputs};

/// Main Controller Struct
pub struct PlantController {
    config: ControlConfig,
    
    // PID Controllers (piddiy)
    pid_air_temp: PidController<Number, Number>,
    pid_peltier_temp: PidController<Number, Number>,
    pid_hum_cold: PidController<Number, Number>,
    pid_fan_temp_outer: PidController<Number, Number>,
    pid_fan_hum_hot: PidController<Number, Number>,

    // Adaptive Tuner
    adaptive_tuner: AdaptiveTuner,

    // Slew Limiters
    fan_inner_speed: Number,
    fan_temp_outer_speed: Number,
    fan_hum_hot_speed: Number,

    // State
    pump_nutrient_active: bool,
    pump_water_active: bool,
    dehumidifier_active: bool,
}

impl PlantController {
    pub fn new(config: ControlConfig) -> Self {
        let mut pid_air_temp = PidController::new();
        config.air_temp.apply_to(&mut pid_air_temp);
        pid_air_temp.compute_fn(PidController::default_compute);

        let mut pid_peltier_temp = PidController::new();
        config.peltier_temp_heat.apply_to(&mut pid_peltier_temp); // Default to heat gains
        pid_peltier_temp.compute_fn(PidController::default_compute);

        let mut pid_hum_cold = PidController::new();
        config.hum_cold_side.apply_to(&mut pid_hum_cold);
        pid_hum_cold.compute_fn(PidController::default_compute);

        let mut pid_fan_temp_outer = PidController::new();
        config.fan_temp_outer.apply_to(&mut pid_fan_temp_outer);
        pid_fan_temp_outer.compute_fn(PidController::default_compute);

        let mut pid_fan_hum_hot = PidController::new();
        config.fan_hum_hot.apply_to(&mut pid_fan_hum_hot);
        pid_fan_hum_hot.compute_fn(PidController::default_compute);

        Self {
            config,
            pid_air_temp,
            pid_peltier_temp,
            pid_hum_cold,
            pid_fan_temp_outer,
            pid_fan_hum_hot,
            adaptive_tuner: AdaptiveTuner::new(300), // Update every 300 steps (e.g., 30 seconds at 10Hz)
            fan_inner_speed: Number::from_num(0),
            fan_temp_outer_speed: Number::from_num(0),
            fan_hum_hot_speed: Number::from_num(0),
            pump_nutrient_active: false,
            pump_water_active: false,
            dehumidifier_active: true,
        }
    }

    pub fn update_config(&mut self, new_config: ControlConfig) {
        self.config = new_config;
        new_config.air_temp.apply_to(&mut self.pid_air_temp);
        new_config.hum_cold_side.apply_to(&mut self.pid_hum_cold);
        new_config.fan_temp_outer.apply_to(&mut self.pid_fan_temp_outer);
        new_config.fan_hum_hot.apply_to(&mut self.pid_fan_hum_hot);
    }



    // determine_targets removed, targets passed in step


    fn control_humidity(&mut self, sensors: &SensorData, targets: &TargetState) -> (Number, Number) {
        let current_hum = if let Some(reading) = sensors.internal {
            Number::from_num(reading.hum)
        } else {
            return (Number::from_num(0), Number::from_num(0));
        };

        let hum_cold_temp = if let Some(ntc_temps) = sensors.ntc_temps {
            ntc_temps[NTC_PELTIER_HUM_COLD]
        } else {
            return (Number::from_num(0), Number::from_num(0));
        };

        let setpoint = Number::from_num(targets.humidity);
        
        let hysteresis = Number::from_num(10);
        if current_hum > setpoint + hysteresis {
            self.dehumidifier_active = true;
        } else if current_hum < setpoint - hysteresis {
            self.dehumidifier_active = false;
        }

        if self.dehumidifier_active {
            self.pid_hum_cold.set_point(self.config.hum_cold_target);
            
            let effort = self.pid_hum_cold.compute(hum_cold_temp);
            let pwm = (-effort).clamp(Number::from_num(0), Number::from_num(255));
            (pwm, Number::from_num(1))
        } else {
            (Number::from_num(0), Number::from_num(0))
        }
    }

    fn control_ventilation(&self, sensors: &SensorData, targets: &TargetState) -> bool {
        targets.vent_on
    }

    fn control_soil_moisture(&mut self, sensors: &SensorData) -> bool {
        let moisture = if let Some(val) = sensors.soil_moisture {
            Number::from_num(val)
        } else {
            self.pump_nutrient_active = false;
            return false;
        };
        
        // Capacitive Sensor: High Value = Dry, Low Value = Wet
        if moisture > self.config.soil_high_threshold {
            self.pump_nutrient_active = true; // Too Dry -> Start Pump
        } else if moisture < self.config.soil_low_threshold {
            self.pump_nutrient_active = false; // Wet Enough -> Stop Pump
        }
        
        self.pump_nutrient_active
    }

    fn control_ec_mode(&mut self, sensors: &SensorData) -> bool {
        let ec = if let Some(val) = sensors.ec_level {
            Number::from_num(val)
        } else {
            self.pump_water_active = false;
            return false;
        };
        
        if ec > self.config.ec_high_threshold {
            self.pump_water_active = true; // Too salty, add water
        } else if ec < self.config.ec_low_threshold {
            self.pump_water_active = false; // OK, stop water
        }
        
        self.pump_water_active
    }

    fn control_temperature(
        &mut self, 
        sensors: &SensorData, 
        targets: &TargetState, 
        hum_peltier_pwm: Number, 
        vent_on: bool
    ) -> (Number, Number) {
        let internal_temp = if let Some(reading) = sensors.internal {
            reading.temp
        } else {
            return (Number::from_num(0), Number::from_num(0));
        };

        let external_temp = if let Some(reading) = sensors.external {
            reading.temp
        } else {
            Number::from_num(25.0) // Assume standard ambient if missing
        };

        let ntc_temps = if let Some(temps) = sensors.ntc_temps {
            temps
        } else {
            return (Number::from_num(0), Number::from_num(0));
        };
        let peltier_inner_temp = ntc_temps[NTC_PELTIER_INNER];

        let target_temp = targets.temp;

        let ff_hum = self.config.k_ff_hum * hum_peltier_pwm;
        let vent_factor = if vent_on { Number::from_num(1) } else { Number::from_num(0) };
        let ff_vent = self.config.k_ff_vent * vent_factor * (target_temp - external_temp);

        self.pid_air_temp.set_point(target_temp);
        let effort_pid = self.pid_air_temp.compute(internal_temp);
        
        // Adaptive Tuning Update
        let error = target_temp - internal_temp;
        self.adaptive_tuner.update(error, &mut self.config.air_temp);
        self.config.air_temp.apply_to(&mut self.pid_air_temp);

        let total_effort = effort_pid + ff_hum + ff_vent;
        let fan_effort = effort_pid + ff_vent;

        let fan_target = self.config.fan_base_day + (self.config.k_fan_effort * fan_effort.abs() / Number::from_num(255));
        
        // Scale factor adjusted for 0-255 input (approx 0.3 / 2.55 ~= 0.12)
        let target_peltier_temp = Number::from_num(30) + (total_effort * Number::from_num(0.12)); 
        let target_peltier_temp = target_peltier_temp.clamp(Number::from_num(0), Number::from_num(60));

        if target_peltier_temp > peltier_inner_temp {
            self.config.peltier_temp_heat.apply_to(&mut self.pid_peltier_temp);
        } else {
            self.config.peltier_temp_cool.apply_to(&mut self.pid_peltier_temp);
        }

        self.pid_peltier_temp.set_point(target_peltier_temp);
        let peltier_pwm = self.pid_peltier_temp.compute(peltier_inner_temp);
        let peltier_pwm = peltier_pwm.clamp(Number::from_num(-255), Number::from_num(255));

        (peltier_pwm, fan_target)
    }

    fn control_aux_fans(&mut self, sensors: &SensorData) -> (Number, Number) {
        if let Some(ntc_temps) = sensors.ntc_temps {
            let t_peltier_inner = ntc_temps[NTC_PELTIER_INNER];
            let t_peltier_outer = ntc_temps[NTC_PELTIER_OUTER];
            let t_hum_cold = ntc_temps[NTC_PELTIER_HUM_COLD];
            let t_hum_hot = ntc_temps[NTC_PELTIER_HUM_HOT];

            // Calculate differences (Hot - Cold)
            let diff_temp = (t_peltier_outer - t_peltier_inner).abs();
            let diff_hum = (t_hum_hot - t_hum_cold).abs();

            self.pid_fan_temp_outer.set_point(self.config.peltier_temp_diff_target);
            
            let effort_temp = self.pid_fan_temp_outer.compute(diff_temp - self.config.peltier_temp_diff_target);
            let effort_hum = self.pid_fan_hum_hot.compute(diff_hum - self.config.peltier_temp_diff_target);

            (effort_temp, effort_hum)
        } else {
            (Number::from_num(0), Number::from_num(0))
        }
    }

    fn apply_slew_limits(&mut self, fan_inner_target: Number, fan_temp_outer_target: Number, fan_hum_hot_target: Number) {
        let alpha = Number::from_num(0.1);
        self.fan_inner_speed = self.fan_inner_speed * (Number::from_num(1) - alpha) + fan_inner_target * alpha;
        self.fan_temp_outer_speed = self.fan_temp_outer_speed * (Number::from_num(1) - alpha) + fan_temp_outer_target * alpha;
        self.fan_hum_hot_speed = self.fan_hum_hot_speed * (Number::from_num(1) - alpha) + fan_hum_hot_target * alpha;
    }

    fn post_process(
        &mut self,
        peltier_temp_pwm: Number,
        hum_peltier_pwm: Number,
        vent_on: bool,
        light_intensity: u8,
        pump_nutrient_active: bool,
        pump_water_active: bool,
        sensors: &SensorData
    ) -> ActuatorOutputs {
        let max_fan = Number::from_num(self.config.max_fan_speed);
        let fan_inner: u8 = self.fan_inner_speed.clamp(Number::from_num(0), max_fan).to_num();
        
        let fan_temp_outer_speed: u8 = self.fan_temp_outer_speed.clamp(Number::from_num(0), max_fan).to_num();
        let fan_hum_hot_speed: u8 = self.fan_hum_hot_speed.clamp(Number::from_num(0), max_fan).to_num();

        // H-Bridge Logic for Temperature Peltier
        // dir=1 (Heat): PWM 100% -> Off, PWM 0% -> On (Active Low)
        // dir=0 (Cool): PWM 100% -> On, PWM 0% -> Off (Active High)
        
        // peltier_temp_pwm comes in as signed Number (-255 to 255)
        // Positive = Heat, Negative = Cool
        
        let (temp_dir, temp_pwm_u8) = if let Some(ntc_temps) = sensors.ntc_temps {
            let temp = ntc_temps[NTC_PELTIER_INNER];
            let mut magnitude = peltier_temp_pwm.abs().clamp(Number::from_num(0), Number::from_num(255)).to_num::<u8>();
            if temp > Number::from_num(60) {
                magnitude = 0;
            }
            if peltier_temp_pwm >= Number::from_num(0) {
                (true, 255 - magnitude)
            } else {
                (false, magnitude)
            }
        } else {
            // Sensor Fail -> OFF
            (false, 0)
        };

        let peltier_hum_u8 = if let Some(ntc_temps) = sensors.ntc_temps {
            let temp = ntc_temps[NTC_PELTIER_HUM_HOT];
            if temp > Number::from_num(70) {
                0
            } else {
                hum_peltier_pwm.to_num::<u8>()
            }
        } else {
            0
        };

        ActuatorOutputs {
            peltier_temp_pwm: temp_pwm_u8,
            peltier_temp_dir: temp_dir,
            peltier_hum_pwm: peltier_hum_u8,
            fan_inner_speed: fan_inner,
            fan_temp_outer_speed,
            fan_hum_hot_speed,
            fan_vent_on: vent_on,
            led_intensity: light_intensity,
            pump_nutrient: pump_nutrient_active,
            pump_water: pump_water_active,
        }
    }



    pub async fn step(&mut self, sensors: &SensorData, targets: TargetState) -> ActuatorOutputs {
        let (hum_peltier_pwm, hum_fan_speed) = self.control_humidity(sensors, &targets);
        let vent_on = self.control_ventilation(sensors, &targets);
        let pump_nutrient_active = self.control_soil_moisture(sensors);
        let pump_water_active = self.control_ec_mode(sensors);
        let (fan_temp_outer_effort, fan_hum_hot_effort) = self.control_aux_fans(sensors);

        let (peltier_temp_pwm, fan_inner_target) = self.control_temperature(
            sensors, 
            &targets, 
            hum_peltier_pwm, 
            vent_on
        );

        self.apply_slew_limits(fan_inner_target, fan_temp_outer_effort, fan_hum_hot_effort);

        self.post_process(
            peltier_temp_pwm, 
            hum_peltier_pwm, 
            vent_on, 
            targets.light_intensity,
            pump_nutrient_active,
            pump_water_active,
            sensors
        )
    }
}




