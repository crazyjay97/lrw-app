use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use embassy_time::Duration;
use heapless::String;

use crate::{
    info,
    serial::{
        send_command, Command, GetAppEuiResult, GetDevAddrResult, GetDevEuiResult, GetVerResult,
    },
};

pub static LORAWAN: Mutex<ThreadModeRawMutex, Option<LoRaWAN>> = Mutex::new(None);

pub struct LoRaWAN {
    pub deveui: Option<String<16>>,
    pub appeui: Option<String<16>>,
    pub appkey: Option<String<32>>,
    pub devaddr: Option<String<8>>,
    pub nwkskey: Option<String<32>>,
    pub appskey: Option<String<32>>,
    pub version: Option<String<128>>,
    pub class: Class,
    pub join_type: JoinType,
    pub state: State,
}

pub enum Class {
    A,
    B,
    C,
}

impl Class {
    pub fn as_str(&self) -> &'static str {
        match self {
            Class::A => "a",
            Class::B => "b",
            Class::C => "c",
        }
    }
}

pub enum JoinType {
    Otaa,
    Abp,
}

impl JoinType {
    pub fn as_str(&self) -> &'static str {
        match self {
            JoinType::Otaa => "otaa",
            JoinType::Abp => "abp",
        }
    }
}

pub enum State {
    Join,
    Joined,
}

impl State {
    pub fn as_str(&self) -> &'static str {
        match self {
            State::Join => "join",
            State::Joined => "joined",
        }
    }
}

pub async fn init_lorawan_info() -> Result<(), ()> {
    let dev_eui: GetDevEuiResult =
        send_command(Command::GetDevEui, Duration::from_millis(300)).await?;
    let dev_addr: GetDevAddrResult =
        send_command(Command::GetDevAddr, Duration::from_millis(300)).await?;
    info!("dev_addr: {:?}", dev_addr.0);
    let app_eui: GetAppEuiResult =
        send_command(Command::GetAppEui, Duration::from_millis(300)).await?;
    info!("app_eui: {:?}", app_eui.0);
    let ver: GetVerResult = send_command(Command::GetVer, Duration::from_millis(300)).await?;
    info!("ver: {:?}", ver.0);
    let mut appkey: String<32> = String::new();
    appkey.push_str(dev_eui.0.as_str())?;
    appkey.push_str(dev_eui.0.as_str())?;
    let info = LoRaWAN {
        deveui: Some(dev_eui.0),
        appeui: Some(app_eui.0),
        appkey: Some(appkey),
        devaddr: Some(dev_addr.0),
        nwkskey: None,
        appskey: None,
        class: Class::A,
        join_type: JoinType::Otaa,
        state: State::Join,
        version: Some(ver.0),
    };
    *LORAWAN.lock().await = Some(info);
    Ok(())
}
