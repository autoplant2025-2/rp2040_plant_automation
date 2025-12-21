use alloc::rc::Rc;
use embassy_executor::Spawner;
use embassy_rp::flash::{Flash, Async};
use embassy_rp::Peri;
use embassy_rp::peripherals::{DMA_CH0, DMA_CH1, FLASH};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use sequential_storage::map::{fetch_item, store_item};
use sequential_storage::cache::NoCache;
use crate::config_manager::ConfigManager;
use crate::config_types::{CalibrationData, DeviceSettings, PlantConfiguration};

const FLASH_SIZE: usize = 2 * 1024 * 1024;

// Flash range for persistence (Last 64KB of 2MB flash)
// 2MB = 0x200000
// Start = 0x200000 - 0x10000 = 0x1F0000
const FLASH_RANGE_START: u32 = 0x1F0000;
const FLASH_RANGE_END: u32 = 0x200000;

// Keys for storage
const KEY_CALIBRATION: u8 = 1;
const KEY_SETTINGS: u8 = 2;
const KEY_PLANT_CONFIG: u8 = 3;

pub struct PersistenceManager<'d> {
    flash: Flash<'d, FLASH, Async, FLASH_SIZE>,
    flash_range: core::ops::Range<u32>,
}

impl<'d> PersistenceManager<'d> {
    pub fn new(flash: Flash<'d, FLASH, Async, FLASH_SIZE>) -> Self {
        Self {
            flash,
            flash_range: FLASH_RANGE_START..FLASH_RANGE_END,
        }
    }

    pub async fn save_calibration(&mut self, data: &CalibrationData) -> Result<(), ()> {
        let mut buf = [0u8; 1024]; // Buffer for serialization
        let bytes = postcard::to_slice(data, &mut buf).map_err(|_| ())?;
        
        let slice: &[u8] = &*bytes;
        store_item::<u8, &[u8], _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut [0u8; 128], // Scratch buffer
            &KEY_CALIBRATION,
            &slice,
        ).await.map_err(|_| ())
    }

    pub async fn load_calibration(&mut self) -> Option<CalibrationData> {
        let mut buf = [0u8; 1024];
        
        let item = fetch_item(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &KEY_CALIBRATION,
        ).await.ok()??; // Result -> Option -> Option (if None found)

        postcard::from_bytes(item).ok()
    }

    pub async fn save_settings(&mut self, data: &DeviceSettings) -> Result<(), ()> {
        let mut buf = [0u8; 512];
        let bytes = postcard::to_slice(data, &mut buf).map_err(|_| ())?;
        
        let slice: &[u8] = &*bytes;
        store_item::<u8, &[u8], _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut [0u8; 128],
            &KEY_SETTINGS,
            &slice,
        ).await.map_err(|_| ())
    }

    pub async fn load_settings(&mut self) -> Option<DeviceSettings> {
        let mut buf = [0u8; 512];
        
        let item = fetch_item(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &KEY_SETTINGS,
        ).await.ok()??;

        postcard::from_bytes(item).ok()
    }

    pub async fn save_plant_config(&mut self, data: &PlantConfiguration) -> Result<(), ()> {
        // Plant config can be large (8kB script + metadata)
        // We need a larger buffer.
        // WARNING: 9KB on stack might be too much. Consider using heap or splitting.
        // Since we have 'alloc', let's use a heap-allocated buffer.
        
        let mut buf = [0u8; 4096]; 
        let bytes = postcard::to_slice(data, &mut buf).map_err(|_| ())?;
        
        let slice: &[u8] = &*bytes;
        store_item::<u8, &[u8], _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut [0u8; 128],
            &KEY_PLANT_CONFIG,
            &slice,
        ).await.map_err(|_| ())
    }

    pub async fn load_plant_config(&mut self) -> Option<PlantConfiguration> {
        let mut buf = [0u8; 4096];
        
        let item = fetch_item(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &KEY_PLANT_CONFIG,
        ).await.ok()??;

        postcard::from_bytes(item).ok()
    }
}