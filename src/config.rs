use embassy_stm32::flash::{self, Async, Flash};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use heapless::Vec;

use crate::{info, FLASH};

const FLASH_START_ADDRESS: u32 = 0x26000;
const FLASH_PAGE_SIZE: u32 = 2048;

pub static CONFIG: Mutex<ThreadModeRawMutex, Option<Config>> = Mutex::new(None);

pub struct Config {
    pub code: Vec<u8, 16>,
}

impl Config {
    pub fn to_bytes(&self) -> Vec<u8, 1024> {
        let mut buf = Vec::new();
        let _ = buf.extend_from_slice(&self.code);
        buf
    }
}
//pub async fn write_config(conf: &Config) -> Result<(), ()>
//pub async fn read_config() -> Result<Config, ()>

pub async fn write_config(conf: &Config) -> Result<(), ()> {
    let mut flash = FLASH.lock().await;
    let mut flash = flash.as_mut().unwrap();
    let mut f = Flash::new_blocking(&mut flash).into_blocking_regions().bank2_region;
    f.blocking_erase(FLASH_START_ADDRESS, FLASH_START_ADDRESS + FLASH_PAGE_SIZE)
         .map_err(|_| ())?;

    //let mut buf = conf.to_bytes();
    let mut buf = [31;2048];

    f.blocking_write(FLASH_START_ADDRESS, &mut buf)
        .map_err(|e| {
            info!("e: {:?}",e);
            ()
        })?;
    Ok(())
}

pub async fn read_config() -> Result<Config, ()> {
    let mut flash = FLASH.lock().await;
    let flash = flash.as_mut().unwrap();
    let mut f = Flash::new_blocking(flash).into_blocking_regions().bank2_region;
    let mut buf = [0;2048];
    f.blocking_read(FLASH_START_ADDRESS, &mut buf)
        .map_err(|_| ())?;
    let mut code = Vec::new();
    let _ = code.extend_from_slice(&buf);
    Ok(Config { code })
}

pub async fn init() -> Result<(), ()> {
    let config = Config{
        code: Vec::from_slice(b"hello world").unwrap(),
    };
    let _ = write_config(&config).await?;
    info!("6666666666666666666");
    let config = read_config().await?;
    info!("6666666666666666666");
    info!("66666666666666666666 {:?}",core::str::from_utf8(&config.code).unwrap());
    *CONFIG.lock().await = Some(config);
    Ok(())
}
