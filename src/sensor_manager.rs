use embassy_embedded_hal::shared_bus;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embedded_hal_async::i2c::I2c;
use shared_bus::asynch::i2c::I2cDevice;

use crate::control::Number;
use self::sensor_filter::MultiChannelKalmanFilter;
use temp_hum_sensor_async::sht20::Sht20;
use temp_hum_sensor_async::aht20::Aht20;
use temp_hum_sensor_async::TempHumSensor;


pub mod sensor_filter;



const PCF8591_VREF: f32 = 3.3;

// NTC Defaults (Common 10k Module)


/// Combined Temperature and Humidity Reading
#[derive(Clone, Copy, Debug, Default)]
pub struct TempHumReading {
    pub temp: Number,
    pub hum: u8,
}

/// Collected sensor data from all hardware
#[derive(Clone, Debug, Default)]
pub struct SensorData {
    pub internal: Option<TempHumReading>,
    pub external: Option<TempHumReading>,
    pub ntc_temps: Option<[Number; 4]>,
    pub soil_moisture: Option<Number>,
    pub ec_level: Option<Number>,
    pub co2_level: Option<Number>,
}

#[derive(Clone, Copy, Debug)]
pub struct CalibrationConfig {
    // EC (TDS)
    pub ec_k_value: f32,
}

impl Default for CalibrationConfig {
    fn default() -> Self {
        Self {
            ec_k_value: 1.0,
        }
    }
}

/// Manages sensor acquisition, oversampling, and EKF state estimation
use embassy_rp::adc::{Adc, Channel, Async};

use embassy_rp::gpio::Pull;
use embassy_rp::Peri;

pub struct SensorManager<'a, I2C> {
    // Single Multi-Channel Filter
    filter: MultiChannelKalmanFilter,
    
    // Calibration
    calibration: CalibrationConfig,

    // SHT20
    sht20: Sht20<I2cDevice<'a, NoopRawMutex, I2C>>,
    // AHT20
    aht20: Aht20<I2cDevice<'a, NoopRawMutex, I2C>>,
    
    // ADC
    adc: Adc<'a, Async>,
    pin_soil: Channel<'a>,
    pin_ec: Channel<'a>,
}

impl<'a, I2C> SensorManager<'a, I2C>
where
    I2C: I2c<Error = embassy_rp::i2c::Error>,
{
    pub fn new(
        bus: &'a Mutex<NoopRawMutex, I2C>,
        adc: Adc<'a, Async>,
        pin_soil: Channel<'a>,
        pin_ec: Channel<'a>,
    ) -> Self {
        // Initialize filter with default noise parameters
        let q = 0.01;
        let r_temp = 40.0;
        let r_hum = 5.0;
        let r_adc = 0.2; // Faster response (was 5.0)

        let initial_values = [
            25.0, 50.0, 25.0, 50.0,
            25.0, 25.0, 25.0, 25.0,
            4095.0, 0.0, // Soil starts at Max (Safety - No Tray), EC at 0
        ];

        let measurement_noises = [
            r_temp, r_hum, r_temp, r_hum,
            r_temp, r_temp, r_temp, r_temp,
            r_adc, r_adc,
        ];

        Self {
            filter: MultiChannelKalmanFilter::new(initial_values, q, measurement_noises),
            calibration: CalibrationConfig::default(),
            sht20: Sht20::new(I2cDevice::new(bus)),
            aht20: Aht20::new(I2cDevice::new(bus)),
            adc,
            pin_soil,
            pin_ec,
        }
    }

    pub async fn step(&mut self) -> SensorData {
        let mut data = SensorData::default();
        
        let sht = self.read_sht20_raw().await;
        let aht = self.read_aht20_raw().await;
        // Mocked PCF8591
        let ntcs = Some([30.0, 30.0, 5.0, 70.0]); 
        let soil = self.read_adc_soil().await;
        
        let ec_temp = sht.map(|r| r.temp).unwrap_or(25.0);
        let ec = self.read_adc_ec(ec_temp).await; 

        let ntc_temps_or_default = ntcs.unwrap_or([0.0; 4]);

        let measurements = [
            sht.map(|r| r.temp).unwrap_or(0.0),
            sht.map(|r| r.hum).unwrap_or(0.0),
            aht.map(|r| r.temp).unwrap_or(0.0),
            aht.map(|r| r.hum).unwrap_or(0.0),
            ntc_temps_or_default[0],
            ntc_temps_or_default[1],
            ntc_temps_or_default[2],
            ntc_temps_or_default[3],
            soil.unwrap_or(0.0),
            ec.unwrap_or(0.0),
        ];

        let filtered = self.filter.update(measurements);

        data.internal = sht.map(|_| TempHumReading {
            temp: Number::from_num(filtered[0]),
            hum: filtered[1] as u8,
        });

        data.external = aht.map(|_| TempHumReading {
            temp: Number::from_num(filtered[2]),
            hum: filtered[3] as u8,
        });
        
        data.ntc_temps = if ntcs.is_some() {
            Some([4, 5, 6, 7].map(|i| Number::from_num(filtered[i])))
        } else {
            None
        };

        data.soil_moisture = soil.map(|_| Number::from_num(filtered[8]));
        data.ec_level = ec.map(|_| Number::from_num(filtered[9]));
        
        data
    }

    async fn read_sht20_raw(&mut self) -> Option<temp_hum_sensor_async::Reading> {
        match self.sht20.read(&mut embassy_time::Delay).await {
            Ok(reading) => Some(reading),
            Err(_) => None
        }
    }

    async fn read_aht20_raw(&mut self) -> Option<temp_hum_sensor_async::Reading> {
        match self.aht20.read(&mut embassy_time::Delay).await {
            Ok(reading) => Some(reading),
            Err(_) => None
        }
    }

    async fn read_adc_soil(&mut self) -> Option<f32> { 
        let raw = self.adc.read(&mut self.pin_soil).await;
        match raw {
            Ok(val) => Some(val as f32),
            Err(_) => None
        }
    } 
    
    async fn read_adc_ec(&mut self, temp_c: f32) -> Option<f32> { 
        let raw = self.adc.read(&mut self.pin_ec).await;
        match raw {
            Ok(val) => {
                 // Map 12-bit (4095) to suitable range or use voltage
                 let adc_8bit = (val >> 4) as u8;
                 convert_ec(adc_8bit, temp_c, self.calibration.ec_k_value)
            },
            Err(_) => None
        }
    }
}



fn convert_ec(adc: u8, temp_c: f32, k_value: f32) -> Option<f32> {
    let v_raw = adc as f32 * (PCF8591_VREF / 255.0);
    let v_comp = v_raw / (1.0 + 0.02 * (temp_c - 25.0));
    let v3 = v_comp * v_comp * v_comp;
    let v2 = v_comp * v_comp;
    let tds_ppm = (133.42 * v3 - 255.86 * v2 + 857.39 * v_comp) * 0.5 * k_value;
    Some(tds_ppm.max(0.0))
}

use alloc::rc::Rc;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
pub type SharedSensorData = Rc<Mutex<CriticalSectionRawMutex, SensorData>>;

#[embassy_executor::task]
pub async fn sensor_task(
    i2c: embassy_rp::i2c::I2c<'static, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Async>,
    adc: Adc<'static, Async>,
    pin_soil: Peri<'static, embassy_rp::peripherals::PIN_27>,
    pin_ec: Peri<'static, embassy_rp::peripherals::PIN_26>,
    shared_data: SharedSensorData,
) {
    let bus = Mutex::new(i2c);
    let ch_soil = Channel::new_pin(pin_soil, Pull::None);
    let ch_ec = Channel::new_pin(pin_ec, Pull::None);
    let mut manager = SensorManager::new(&bus, adc, ch_soil, ch_ec);
    
    loop {
        let data = manager.step().await;
        {
            let mut shared = shared_data.lock().await;
            *shared = data;
        }
        
        embassy_time::Timer::after_millis(100).await;
    }
}

impl defmt::Format for CalibrationConfig {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "CalibrationConfig {{ ec_k_value: {} }}", self.ec_k_value);
    }
}

impl defmt::Format for TempHumReading {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "TempHumReading {{ temp: {}, hum: {} }}", 
            self.temp.to_num::<f32>(), self.hum);
    }
}

impl defmt::Format for SensorData {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "SensorData {{ internal: {}, external: {}, soil: {}, ec: {} }}",
            self.internal,
            self.external,
            // Simple handling for Option<Number> by converting to f32 or 0.0
            match self.soil_moisture { Some(n) => n.to_num::<f32>(), None => -1.0 },
            match self.ec_level { Some(n) => n.to_num::<f32>(), None => -1.0 }
            // Skipping NTC/CO2 for brevity/simplicity in logs to avoid complex formatting logic
        );
    }
}
