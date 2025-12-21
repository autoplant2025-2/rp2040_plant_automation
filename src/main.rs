#![no_std]
#![no_main]
extern crate alloc;

#[macro_export]
macro_rules! modpub {
    ($name:ident) => {
        mod $name;
        pub use $name::*;
    };
}


//defmt
use defmt_rtt as _;

//panic handling
use panic_probe as _;
use alloc::rc::Rc;
use core::mem::MaybeUninit;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use chrono::Timelike;
use embassy_rp::clocks::{ClockConfig, CoreVoltage};
use embassy_rp::config::Config;
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_sync::mutex::Mutex;
//pub use defmt::;
use embassy_time::Timer;
use embedded_hal_async::delay::DelayNs;
use talc::{ClaimOnOom, Talc, Talck};
use crate::config_manager::init_persistence_config;

mod main_ui;
pub mod control;
pub mod sensor_manager;
pub mod hardware_manager;
pub mod persistence_manager;
pub mod config_manager;
pub mod config_types;
pub mod time_manager;
mod ui;
pub mod network;

use embassy_rp::gpio::{Output, Level};
use embassy_rp::pwm::{Pwm, Config as PwmConfig};
use embassy_rp::adc::{Adc, Config as AdcConfig};
use crate::hardware_manager::{HardwareManager, SharedActuatorState, ActuatorOutputs, HardwareInterface};
use crate::control::{PlantController, ControlConfig};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

static mut ARENA: MaybeUninit<[u8; 1024 * 160]> = MaybeUninit::uninit();

// 힙 allocator
#[allow(static_mut_refs)]
#[global_allocator]
static ALLOCATOR: Talck<spin::Mutex<()>, ClaimOnOom> = Talc::new(unsafe {
	ClaimOnOom::new(talc::Span::from_array(ARENA.as_ptr().cast_mut()))
}).lock();

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
    I2C0_IRQ => embassy_rp::i2c::InterruptHandler<embassy_rp::peripherals::I2C0>;
    ADC_IRQ_FIFO => embassy_rp::adc::InterruptHandler;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut cc = ClockConfig::system_freq(200_000_000).unwrap(); //오버클럭의 생활화.....
    cc.core_voltage = CoreVoltage::V1_30;
    let p = embassy_rp::init(Config::new(cc));

    //spawn input handling task
    let Pio {
        mut common, sm0, sm1, irq0, ..
    } = Pio::new(p.PIO0, Irqs);

    let shared_config = init_persistence_config(
        p.FLASH, p.DMA_CH1
    ).await;

    let initial_time = {
        let cfg = shared_config.lock().await;
        let ts = cfg.settings().last_datetime;
        if ts > 0 {
            chrono::DateTime::from_timestamp(ts as i64, 0)
        } else {
            None
        }
    };

    let time_manager = Rc::new(time_manager::TimeManager::new(initial_time));

    // ADC Init
    let adc = Adc::new(p.ADC, Irqs, AdcConfig::default());
    let pin_soil = p.PIN_27; // ADC1 - Water Tray / Soil
    let pin_ec = p.PIN_26;   // ADC0 - EC Sensor

    // Init I2C for Sensors
    let i2c = embassy_rp::i2c::I2c::new_async(p.I2C0, p.PIN_1, p.PIN_0, Irqs, embassy_rp::i2c::Config::default());
    
    let shared_sensor_data: crate::sensor_manager::SharedSensorData = Rc::new(Mutex::new(crate::sensor_manager::SensorData::default()));
    
    spawner.spawn(crate::sensor_manager::sensor_task(i2c, adc, pin_soil, pin_ec, shared_sensor_data.clone()).unwrap());

    // Hardware Peripherals
    // PWMs
    // GPIO16: Peltier PWM (Slice 0 A)
    let peltier_pwm = Pwm::new_output_a(p.PWM_SLICE0, p.PIN_16, PwmConfig::default());
    // GPIO17: Peltier Dir (Output)
    let peltier_dir = Output::new(p.PIN_17, Level::Low);
    
    // GPIO18/19: Fans (Slice 1)
    // GPIO18 (A) = Inner, GPIO19 (B) = Outer
    let fan_pwm = Pwm::new_output_ab(p.PWM_SLICE1, p.PIN_18, p.PIN_19, PwmConfig::default());

    // GPIO20: LED (Slice 2 A)
    // 1000Hz PWM for LED
    // Clock = 125MHz typically.
    // 1kHz = 125M / (div * (top+1))
    // Let div = 2.0 -> (top+1) = 62,500 -> top = 62499
    let mut led_conf = PwmConfig::default();
    led_conf.top = 62499; 
    led_conf.divider = fixed::FixedU16::<fixed::types::extra::U4>::from_num(2.0);
    let led_pwm = Pwm::new_output_a(p.PWM_SLICE2, p.PIN_20, led_conf);

    // GPIO21: Pump Nutrient (Output)
    let pump_nutrient = Output::new(p.PIN_21, Level::Low);
    
    // Mocked/Unused
    let pump_water = Output::new(p.PIN_22, Level::Low); // Arbitrary unused
    let fan_vent = Output::new(p.PIN_28, Level::Low);   // Arbitrary unused

    let mut hardware = HardwareManager::new(
        led_pwm,
        fan_pwm,
        peltier_pwm,
        peltier_dir,
        pump_nutrient,
        pump_water,
        fan_vent
    );

    let shared_actuator_state: SharedActuatorState = Rc::new(Mutex::new(ActuatorOutputs::default()));

    let (wifi_control, net_steck) = network::init_network(
        &spawner,
        shared_config.clone(),
        time_manager.clone(),
        shared_sensor_data.clone(),
        &mut common,
        sm1,
        irq0,
        p.PIN_23,
        p.PIN_25,
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH2
    ).await;

    ui::init_ui(
        &spawner,
        shared_config.clone(),
        wifi_control,
        net_steck,
        time_manager.clone(),
        shared_sensor_data.clone(),
        shared_actuator_state.clone(),
        // # hardwares
        &mut common,
        sm0,
        p.PIN_14,
        p.PIN_15,
        p.PIN_8,
        p.SPI0,
        p.PIN_6,
        p.PIN_7,
        p.DMA_CH0
    );

    let initial_calibration = {
        let cfg = shared_config.lock().await;
        cfg.calibration().clone()
    };
    
    let mut controller = PlantController::new(initial_calibration.pid_config);

    loop {
        // Run control logic every 10 * 100ms = 1s?
        // User said "run control loop every 10 sample of sensors".
        // Sensor loop runs every 100ms. So every 1s.
        
        let mut sample_count = 0;
        
        loop {
            Timer::after_millis(50).await;
            sample_count += 1;
            
            // Decimation
            if sample_count >= 10 {
                sample_count = 0;
                
                let sensors = {
                    let data = shared_sensor_data.lock().await;
                    data.clone()
                };
                
                // Get Targets and Calibration
                let (target_temp, light_intensity, start_hour, end_hour, pid_config) = {
                    let cfg = shared_config.lock().await;
                    let pc = cfg.plant_config();
                    (pc.target_temp, pc.light_intensity, pc.light_start_hour, pc.light_end_hour,
                     cfg.calibration().pid_config) // Copy
                };
                
                // Sync Controller Config (incl. Water Tray Calibration)
                controller.update_config(pid_config);

                // Schedule Logic
                let current_hour = if let Some(dt) = time_manager.get_time() {
                    dt.hour() as u8
                } else {
                    12 
                };
                let is_light_on = if start_hour < end_hour {
                    current_hour >= start_hour && current_hour < end_hour
                } else {
                     current_hour >= start_hour || current_hour < end_hour
                };
                let target_light = if is_light_on { light_intensity } else { 0 };

                use crate::control::TargetState;
                use fixed::types::I16F16;
                let targets = TargetState {
                    temp: I16F16::from_num(target_temp),
                    humidity: 60, 
                    vent_on: true, 
                    light_intensity: target_light,
                };
                
                let outputs = controller.step(&sensors, targets).await;
                
                // Actuate
                hardware.actuate(&outputs).await;
                
                // Update Shared State for UI
                {
                    let mut st = shared_actuator_state.lock().await;
                    *st = outputs;
                }
                
                //defmt::info!("Loop: Sensors: {:?} -> Outputs: {:?}", sensors.internal, outputs);
            }
        }
    }
}


// embassy time의 api를 사용한 딜레이 구현
pub struct EmbassyDelay;

impl DelayNs for EmbassyDelay {
	async fn delay_ns(&mut self, ns: u32) {
		Timer::after_nanos(ns as u64).await
	}
}
