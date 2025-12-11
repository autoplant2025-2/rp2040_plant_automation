use embassy_embedded_hal::shared_bus;
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex};
use embedded_hal_async::i2c::I2c;
use shared_bus::asynch::i2c::I2cDevice;

use crate::control::Number;
use self::sensor_filter::MultiChannelKalmanFilter;
use temp_hum_sensor_async::sht20::Sht20;
use temp_hum_sensor_async::aht20::Aht20;
use temp_hum_sensor_async::TempHumSensor;
use pcf8591_async::Pcf8591;

pub mod sensor_filter;

use num_traits::Float;

const PCF8591_VREF: f32 = 3.3;

// NTC Defaults (Common 10k Module)
const NTC_BETA_DEFAULT: f32 = 3950.0;
const NTC_R_SERIES_DEFAULT: f32 = 10000.0;
const NTC_R_NOMINAL_DEFAULT: f32 = 10000.0;
const NTC_T_NOMINAL_DEFAULT: f32 = 25.0;

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
pub struct SensorManager<'a, I2C> {
    // Single Multi-Channel Filter
    filter: MultiChannelKalmanFilter,
    
    // Calibration
    calibration: CalibrationConfig,

    // SHT20
    sht20: Sht20<I2cDevice<'a, NoopRawMutex, I2C>>,
    // AHT20
    aht20: Aht20<I2cDevice<'a, NoopRawMutex, I2C>>,
    // PCF8591 (NTCs)
    pcf8591: Pcf8591<I2cDevice<'a, NoopRawMutex, I2C>>,
}

impl<'a, I2C> SensorManager<'a, I2C>
where
    I2C: I2c<Error = embassy_rp::i2c::Error>,
{
    pub fn new(bus: &'a Mutex<NoopRawMutex, I2C>) -> Self {
        // Initialize filter with default noise parameters
        let q = 0.01;
        let r_temp = 0.1;
        let r_hum = 1.0;
        let r_adc = 5.0;

        // Initial state guess
        let initial_values = [
            25.0, // SHT Temp
            50.0, // SHT Hum
            25.0, // AHT Temp
            50.0, // AHT Hum
            25.0, 25.0, 25.0, 25.0, // NTCs
            500.0, // Soil
            0.0, // EC
        ];

        // Measurement noise for each channel
        let measurement_noises = [
            r_temp, // SHT Temp
            r_hum,  // SHT Hum
            r_temp, // AHT Temp
            r_hum,  // AHT Hum
            r_temp, r_temp, r_temp, r_temp, // NTCs
            r_adc,  // Soil
            r_adc,  // EC
        ];

        Self {
            filter: MultiChannelKalmanFilter::new(initial_values, q, measurement_noises),
            calibration: CalibrationConfig::default(),
            sht20: Sht20::new(I2cDevice::new(bus)),
            aht20: Aht20::new(I2cDevice::new(bus)),
            pcf8591: Pcf8591::new(I2cDevice::new(bus), 0x48),
        }
    }

    pub async fn step(&mut self) -> SensorData {
        let mut data = SensorData::default();
        
        // Gather raw readings (Option where appropriate)
        let sht = self.read_sht20_raw().await;
        let aht = self.read_aht20_raw().await;
        let ntcs = self.read_pcf8591_ntcs().await;
        let soil = self.read_adc_soil().await;
        
        // Use internal air temp as approximation for water temp if available, else default 25.0
        let ec_temp = sht.map(|r| r.temp).unwrap_or(25.0);
        let ec = self.read_adc_ec(ec_temp).await; 

        let ntc_temps_or_default = ntcs.unwrap_or([0.0; 4]);

        // Construct measurement vector for Kalman Filter (all f32)
        // Use 0.0 for missing values to keep filter running (arbitrary value as requested)
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

        // Update filter
        let filtered = self.filter.update(measurements);

        // Map back to SensorData (Number) - Only populate if raw reading was successful
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

    // Helper methods to get raw f32 for Kalman Filter
    async fn read_sht20_raw(&mut self) -> Option<temp_hum_sensor_async::Reading> {
        match self.sht20.read(&mut embassy_time::Delay).await {
            Ok(reading) => Some(reading),
            Err(_) => {
                defmt::error!("SHT20 Read Failed");
                None
            }
        }
    }

    async fn read_aht20_raw(&mut self) -> Option<temp_hum_sensor_async::Reading> {
        match self.aht20.read(&mut embassy_time::Delay).await {
            Ok(reading) => Some(reading),
            Err(_) => {
                defmt::error!("AHT20 Read Failed");
                None
            }
        }
    }



    async fn read_pcf8591_ntcs(&mut self) -> Option<[f32; 4]> {
        let raw = self.pcf8591.read_all().await.map_err(|_| defmt::error!("PCF8591 read failed")).ok()?;
        let mut ntc_converted = [0.0; 4];
        for i in 0..4 {
            if let Some(converted) = convert_ntc(raw[i]) {
                ntc_converted[i] = converted;
            } else {
                defmt::error!("PCF8591 ntc {} abnormal reading: {}", i, raw[i]);
                return None;
            }
        }
        Some(ntc_converted)
    }

    async fn read_adc_soil(&self) -> Option<f32> { 
        // Placeholder: In real hardware, this would read a specific channel from PCF8591 or internal ADC
        // For now, let's assume it's on PCF8591 Channel 3 (re-using NTC slot for demo?)
        // Or better, just return a dummy value until we define the pin map.
        let adc_val = 150; // Dummy Raw Value
        Some(adc_val as f32)
    } 
    
    async fn read_adc_ec(&self, temp_c: f32) -> Option<f32> { 
        let adc_val = 20; // Dummy
        convert_ec(adc_val, temp_c, self.calibration.ec_k_value)
    }
}

// --- Conversion Helpers ---

fn convert_ntc(adc: u8) -> Option<f32> {
    if !(20..236).contains(&adc) { return None; } // Fault
    
    let v_out = adc as f32 * (PCF8591_VREF / 255.0);
    // Divider: V_out = V_ref * R_ntc / (R_series + R_ntc)  (if NTC is bottom)
    // R_ntc = R_series * V_out / (V_ref - V_out)
    let r_ntc = NTC_R_SERIES_DEFAULT * v_out / (PCF8591_VREF - v_out);
    
    // Steinhart-Hart (Beta variant)
    // 1/T = 1/T0 + (1/B) * ln(R/R0)
    let t0 = NTC_T_NOMINAL_DEFAULT + 273.15;
    let inv_t = (1.0 / t0) + (1.0 / NTC_BETA_DEFAULT) * (r_ntc / NTC_R_NOMINAL_DEFAULT).ln();
    
    Some((1.0 / inv_t) - 273.15)
}

fn convert_ec(adc: u8, temp_c: f32, k_value: f32) -> Option<f32> {
    let v_raw = adc as f32 * (PCF8591_VREF / 255.0);
    
    // Temperature Compensation
    // V_comp = V_raw / (1.0 + 0.02 * (T - 25.0))
    let v_comp = v_raw / (1.0 + 0.02 * (temp_c - 25.0));

    // Keystudio TDS Formula (Cubic)
    // TDS = (133.42*v^3 - 255.86*v^2 + 857.39*v) * 0.5 * K
    let v3 = v_comp * v_comp * v_comp;
    let v2 = v_comp * v_comp;
    
    let tds_ppm = (133.42 * v3 - 255.86 * v2 + 857.39 * v_comp) * 0.5 * k_value;
    
    Some(tds_ppm.max(0.0))
}
