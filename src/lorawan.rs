use core::str::FromStr;

use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use heapless::{String, Vec};

use crate::{
    info,
    serial::{
        send_command, Command, GetAppEuiResult, GetDevAddrResult, GetDevEuiResult, GetVerResult,
        VoidResult,
    },
    BUSY, MODE,
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

pub async fn wait_busy() {
    let busy = BUSY.lock().await;
    if busy.as_ref().unwrap().is_low() {
        Timer::after(Duration::from_millis(500)).await;
    }
}

pub async fn init_lorawan_info() -> Result<(), ()> {
    wait_busy().await;
    let dev_eui: GetDevEuiResult =
        send_command(Command::GetDevEui, Duration::from_millis(300)).await?;
    let dev_addr: GetDevAddrResult =
        send_command(Command::GetDevAddr, Duration::from_millis(300)).await.unwrap_or(GetDevAddrResult{
            0: String::<8>::from_str("00000000").unwrap() 
        });
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

///
/// 进入LoRaWAN 透传模式

/// 9. mode set low
pub async fn into_lorawan_mode() {
    //MODE.lock().await.as_mut().unwrap().set_low();
    info!("into lorawan mode");
}

///
/// 1.  恢复出厂
/// 2.  复位
/// 3.  at+band=6      //配置成异频
/// 4.  at+chmask=00FF //配置8-15信道掩码
/// 5.  读取deveui
/// 6.  配置appeui = deveui
/// 7.  配置appkey 固定前缀+deveui
/// 8.  配置class c  
/// 9.  at+rx2=0,505300000 // 配置RX2 DR0, 505300000
/// 10. at+devaddr=(组播地址,四个字节),4,0,(组播Appskey),(组播Nwkskey)
/// 11. at+save\r\n
/// 12. at+reset\r\n
pub async fn factory() {
    MODE.lock().await.as_mut().unwrap().set_high();
    // 1.
    let _: Result<VoidResult, ()> =
        send_command(Command::Factory, Duration::from_millis(2000)).await;
    // 2.
    let _: Result<VoidResult, ()> = send_command(Command::Reset, Duration::from_millis(100)).await;
    // 3.
    let _: Result<VoidResult, ()> =
        send_command(Command::SetBand(6), Duration::from_millis(100)).await;
    // 4.
    let _: Result<VoidResult, ()> = send_command(
        Command::SetChmask(String::<32>::from_str("00FF").unwrap()),
        Duration::from_millis(100),
    )
    .await;
    // 5.
    let deveui = {
        let info = LORAWAN.lock().await;
        info.as_ref().unwrap().deveui.as_ref().unwrap().clone()
    };
    info!("deveui: {}", deveui);
    let appeui = deveui.clone();
    let mut appkey = String::<32>::from_str(deveui.as_str()).unwrap();
    info!("appkey: {}", appkey);
    let _ = appkey.push_str(deveui.as_str());
    // 6.
    let _: Result<VoidResult, ()> =
        send_command(Command::SetAppEui(appeui), Duration::from_millis(100)).await;
    // 7.
    let _: Result<VoidResult, ()> =
        send_command(Command::SetAppkey(appkey), Duration::from_millis(100)).await;
    let _: Result<VoidResult, ()> = send_command(Command::Debug, Duration::from_millis(100)).await;
    // 8.
    let _: Result<VoidResult, ()> =
        send_command(Command::SetClassC, Duration::from_millis(100)).await;
    // 8.
    let _: Result<VoidResult, ()> =
        send_command(Command::SetRx2(0, 505300000), Duration::from_millis(100)).await;
    // 8.
    let _: Result<VoidResult, ()> = send_command(
        Command::SetGroupDevAddr(
            String::<8>::from_str("F8D4A3B1").unwrap(),
            String::<32>::from_str("1F2E3D4C5B6A798087D2C3F4A5B6C7D8").unwrap(),
            String::<32>::from_str("9A8B7C6D5E4F3A2B1C0D9E8F7A6B5C4D").unwrap(),
        ),
        Duration::from_millis(100),
    )
    .await;
    let _: Result<VoidResult, ()> = send_command(Command::Save, Duration::from_millis(100)).await;
    let _: Result<VoidResult, ()> = send_command(Command::Reset, Duration::from_millis(100)).await;
}
