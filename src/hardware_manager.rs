
pub struct ActuatorOutputs {
    pub peltier_temp_dir: bool, // true = Heat, false = Cool
    pub peltier_temp_pwm: u8,
    pub peltier_hum_pwm: u8,
    pub fan_inner_speed: u8,
    pub fan_temp_outer_speed: u8,
    pub fan_hum_hot_speed: u8,
    pub fan_vent_on: bool,
    pub led_intensity: u8,
    pub pump_nutrient: bool,
    pub pump_water: bool,
}

#[allow(async_fn_in_trait)]
pub trait HardwareInterface {
    async fn actuate(&mut self, outputs: &ActuatorOutputs);
}

pub struct HardwareManager {
    // We will add fields later when we implement the actual hardware
}

impl HardwareManager {
    pub fn new() -> Self {
        Self {}
    }
}

impl HardwareInterface for HardwareManager {
    async fn actuate(&mut self, _outputs: &ActuatorOutputs) {
        // TODO: Implement actual actuation
    }
}
