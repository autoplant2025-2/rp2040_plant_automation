use alloc::rc::Rc;
use embassy_executor::Spawner;
use embassy_rp::flash::Flash;
use embassy_rp::Peri;
use embassy_rp::peripherals::{DMA_CH1, FLASH};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use crate::persistence_manager::PersistenceManager;
use crate::config_types::{CalibrationData, DeviceSettings, PlantConfiguration};
use crate::control::ControlConfig;

pub struct ConfigManager<'d> {
    persistence: PersistenceManager<'d>,
    calibration: CalibrationData,
    settings: DeviceSettings,
    plant_config: PlantConfiguration,
}

impl<'d> ConfigManager<'d> {
    pub async fn new(mut persistence: PersistenceManager<'d>) -> Self {
        let calibration = persistence.load_calibration().await.unwrap_or_else(|| CalibrationData {
            pid_config: ControlConfig::default(),
        });
        let settings = persistence.load_settings().await.unwrap_or_default();
        let plant_config = persistence.load_plant_config().await.unwrap_or_default();

        Self {
            persistence,
            calibration,
            settings,
            plant_config,
        }
    }

    pub fn calibration(&self) -> &CalibrationData {
        &self.calibration
    }

    pub fn settings(&self) -> &DeviceSettings {
        &self.settings
    }

    pub fn plant_config(&self) -> &PlantConfiguration {
        &self.plant_config
    }

    pub async fn update_calibration<F>(&mut self, f: F)
    where
        F: FnOnce(&mut CalibrationData),
    {
        f(&mut self.calibration);
        if self.persistence.save_calibration(&self.calibration).await.is_err() {
            defmt::error!("Failed to save calibration");
        }
    }

    pub async fn update_settings<F>(&mut self, f: F)
    where
        F: FnOnce(&mut DeviceSettings),
    {
        f(&mut self.settings);
        if self.persistence.save_settings(&self.settings).await.is_err() {
            defmt::error!("Failed to save settings");
        }
    }

    pub async fn update_plant_config<F>(&mut self, f: F)
    where
        F: FnOnce(&mut PlantConfiguration),
    {
        f(&mut self.plant_config);
        if self.persistence.save_plant_config(&self.plant_config).await.is_err() {
            defmt::error!("Failed to save plant config");
        }
    }
}



pub type SharedConfig = Rc<Mutex<CriticalSectionRawMutex, ConfigManager<'static>>>;
pub async fn init_persistence_config(
    flash: Peri<'static, FLASH>,
    dma: Peri<'static, DMA_CH1>,
) -> SharedConfig {
    let flash = Flash::new(flash, dma);
    let persistence = PersistenceManager::new(flash);
    Rc::new(Mutex::new(ConfigManager::new(persistence).await))


}