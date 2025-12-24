use embassy_rp::pwm::{Pwm, Config as PwmConfig};
use embassy_rp::gpio::Output;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use alloc::rc::Rc;

#[derive(Clone, Copy, Debug, Default, defmt::Format)]
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

pub type SharedActuatorState = Rc<Mutex<CriticalSectionRawMutex, ActuatorOutputs>>;

#[allow(async_fn_in_trait)]
pub trait HardwareInterface {
    async fn actuate(&mut self, outputs: &ActuatorOutputs);
}

pub struct HardwareManager<'a> {
    pub led_pwm: Pwm<'a>,
    pub fan_pwm: Pwm<'a>, // Controls both Inner (A) and Outer (B)
    pub peltier_pwm: Pwm<'a>, 
    pub pump_nutrient: Output<'a>,
    pub pump_water: Output<'a>, 
    pub fan_vent: Output<'a>,   
    pub peltier_dir_pin: Output<'a>,
}

impl<'a> HardwareManager<'a> {
    pub fn new(
        led_pwm: Pwm<'a>,
        fan_pwm: Pwm<'a>,
        peltier_pwm: Pwm<'a>,
        peltier_dir_pin: Output<'a>,
        pump_nutrient: Output<'a>,
        pump_water: Output<'a>,
        fan_vent: Output<'a>,
    ) -> Self {
        Self {
            led_pwm,
            fan_pwm,
            peltier_pwm,
            peltier_dir_pin,
            pump_nutrient,
            pump_water,
            fan_vent,
        }
    }
}

impl<'a> HardwareInterface for HardwareManager<'a> {
    async fn actuate(&mut self, outputs: &ActuatorOutputs) {
        // LED Safety: Max 75%
        // Map 0-255 to 0-62499, clamped at 75% of 62499
        // 75% of 62499 ~= 46874
        // Input is u8 (0-255).
        // val = (u8 * 62499) / 255
        let top = 62499u32;
        let duty = (outputs.led_intensity as u32 * top) / 255;
        let safe_duty = duty.min((top * 3) / 4); // 75% limit
        
        let mut led_conf = PwmConfig::default();
        led_conf.top = 62499; 
        led_conf.compare_a = safe_duty as u16;
        led_conf.divider = fixed::FixedU16::<fixed::types::extra::U4>::from_num(2.0);
        self.led_pwm.set_config(&led_conf);

        // Fans - Share Slice 1
        // Inner = A (GPIO18), Outer = B (GPIO19)
        let mut fan_conf = PwmConfig::default();
        fan_conf.top = 255;
        fan_conf.compare_a = outputs.fan_inner_speed as u16; 
        fan_conf.compare_b = outputs.fan_temp_outer_speed as u16;
        self.fan_pwm.set_config(&fan_conf);

        // Peltier
        let mut final_pwm = outputs.peltier_temp_pwm;

        if outputs.peltier_temp_dir {
            self.peltier_dir_pin.set_high();
            // Invert PWM when Dir is High to drive against the high rail (IN1=1, IN2=0 -> Drive)
            final_pwm = 255 - final_pwm;
        } else {
            self.peltier_dir_pin.set_low();
        }
        
        let mut peltier_conf = PwmConfig::default();
        peltier_conf.top = 255;
        peltier_conf.compare_a = final_pwm as u16; 
        self.peltier_pwm.set_config(&peltier_conf);

        if outputs.pump_nutrient {
            self.pump_nutrient.set_high();
        } else {
            self.pump_nutrient.set_low();
        }

        if outputs.pump_water {
            self.pump_water.set_high();
        } else {
            self.pump_water.set_low();
        }
    }
}
