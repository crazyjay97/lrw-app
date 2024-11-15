use core::fmt::Write;
use core::str::FromStr;

use crate::{fmt::*, lorawan::LoRaWANPackage, AppEvent, DISPLAY_CHANNEL};
use embassy_futures::select::{select, Either};
use embassy_stm32::{
    mode::Async,
    usart::{Uart, UartRx, UartTx},
};
use embassy_sync::{
    blocking_mutex::raw::ThreadModeRawMutex,
    channel::{Channel, Sender},
    mutex::Mutex,
};
use embassy_time::{Duration, Timer};
use heapless::{String, Vec};

type UART1ReadType = Mutex<ThreadModeRawMutex, Option<UartRx<'static, Async>>>;
type UART1WriteType = Mutex<ThreadModeRawMutex, Option<UartTx<'static, Async>>>;

static UART1_READ: UART1ReadType = Mutex::new(None);
static UART1_WRITE: UART1WriteType = Mutex::new(None);

/// 串口数据通信
type SerialDataChannelType = Channel<ThreadModeRawMutex, ([u8; 1024], usize), 16>;
type SerialSender = Sender<'static, ThreadModeRawMutex, ([u8; 1024], usize), 16>;
static CHANNEL: SerialDataChannelType = Channel::new();
static SERIAL_SENDER_CHANNEL: Mutex<ThreadModeRawMutex, Option<SerialSender>> = Mutex::new(None);
pub enum Command {
    GetDevEui,
    GetVer,
    GetDevAddr,
    GetAppEui,
    SetAppkey(String<32>),
    SetAppEui(String<16>),
    SetAppSKey,
    SetNwkSKey,
    Factory,
    SetStatus,
    Save,
    Reset,
    Debug,
    SetClassC,
    SetBand(u8),
    SetChmask(String<32>),
    SetRx2(u8, u32),
    SetDataRate,
    SetGroupDevAddr(String<8>, String<32>, String<32>),
}

pub enum GPIO {
    P0,
    P1,
    P2,
    P3,
}

impl GPIO {
    pub fn to_u8(&self) -> u8 {
        match self {
            GPIO::P0 => 0,
            GPIO::P1 => 1,
            GPIO::P2 => 2,
            GPIO::P3 => 3,
        }
    }
}

pub enum IOState {
    High,
    Low,
}

impl IOState {
    pub fn to_u8(&self) -> u8 {
        match self {
            IOState::High => 1,
            IOState::Low => 0,
        }
    }
}

impl Command {
    fn as_bytes(&self) -> Vec<u8, 128> {
        match self {
            Command::Factory => Vec::<u8, 128>::from_slice(b"at+factory\r\n").unwrap(),
            Command::GetDevEui => Vec::<u8, 128>::from_slice(b"at+deveui?\r\n").unwrap(),
            Command::GetVer => Vec::<u8, 128>::from_slice(b"at+ver?\r\n").unwrap(),
            Command::GetDevAddr => Vec::<u8, 128>::from_slice(b"at+devaddr?\r\n").unwrap(),
            Command::GetAppEui => Vec::<u8, 128>::from_slice(b"at+appeui?\r\n").unwrap(),
            Command::SetAppkey(appkey) => {
                let mut buf = Vec::<u8, 128>::from_slice(b"at+appkey=").unwrap();
                let _ = buf.extend_from_slice(appkey.as_bytes());
                let _ = buf.extend_from_slice(b"\r\n");
                buf
            }
            Command::SetAppEui(appeui) => {
                let mut buf = Vec::<u8, 128>::from_slice(b"at+appeui=").unwrap();
                let _ = buf.extend_from_slice(appeui.as_bytes());
                let _ = buf.extend_from_slice(b"\r\n");
                buf
            }
            Command::SetAppSKey => Vec::<u8, 128>::from_slice(b"at+deveui?\r\n").unwrap(),
            Command::SetNwkSKey => Vec::<u8, 128>::from_slice(b"at+deveui?\r\n").unwrap(),
            Command::Save => Vec::<u8, 128>::from_slice(b"at+save\r\n").unwrap(),
            Command::Reset => Vec::<u8, 128>::from_slice(b"at+reset\r\n").unwrap(),
            Command::Debug => Vec::<u8, 128>::from_slice(b"at+debug=0\r\n").unwrap(),
            Command::SetClassC => Vec::<u8, 128>::from_slice(b"at+class=2\r\n").unwrap(),
            Command::SetBand(baud) => {
                let mut cmd = String::<128>::new();
                let _ = write!(cmd, "at+band={}\r\n", baud);
                Vec::<u8, 128>::from_slice(cmd.as_bytes()).unwrap()
            }
            Command::SetChmask(cmask) => {
                let mut cmd = String::<128>::new();
                let _ = write!(cmd, "at+chmask={}\r\n", cmask.as_str());
                Vec::<u8, 128>::from_slice(cmd.as_bytes()).unwrap()
            }
            Command::SetRx2(dr, freq) => {
                let mut cmd = String::<128>::new();
                let _ = write!(cmd, "at+rx2={},{}\r\n", dr, freq);
                Vec::<u8, 128>::from_slice(cmd.as_bytes()).unwrap()
            }
            Command::SetGroupDevAddr(addr, appskey, nwkwkey) => {
                let mut cmd = String::<128>::new();
                let _ = write!(
                    cmd,
                    "at+devaddr={},4,0,{},{}\r\n",
                    addr.as_str(),
                    appskey.as_str(),
                    nwkwkey.as_str()
                );
                Vec::<u8, 128>::from_slice(cmd.as_bytes()).unwrap()
            }
            Command::SetStatus => Vec::<u8, 128>::from_slice(b"at+status=2,2\r\n").unwrap(),
            Command::SetDataRate => {
                Vec::<u8, 128>::from_slice(b"AT+DATARATE=5,3,50,1,23\r\n").unwrap()
            }
        }
    }
}

pub async fn init(uart: Uart<'static, Async>) {
    let (tx, rx) = uart.split();
    *UART1_WRITE.lock().await = Some(tx);
    *UART1_READ.lock().await = Some(rx);
}

pub async fn uart1_write(data: &[u8]) -> Result<(), ()> {
    let mut lock = UART1_WRITE.lock().await;
    if lock.as_mut().unwrap().write(data).await.is_ok() {
        info!("<<< write success");
    } else {
        info!("<<< write failed");
        return Err(());
    }
    Ok(())
}

#[embassy_executor::task]
pub async fn serial_listen() {
    loop {
        let mut buf = [0u8; 1024];
        let mut lock = UART1_READ.lock().await;
        if let Some(uart) = lock.as_mut() {
            match uart.read_until_idle(&mut buf).await {
                Ok(len) => {
                    info!(">>> receiver len: {}", len);
                    let mut sender = SERIAL_SENDER_CHANNEL.lock().await;
                    if let Some(sender) = sender.as_mut() {
                        info!(">>> sender not none");
                        sender.send((buf, len)).await;
                    } else {
                        info!(">>> sender is none");
                        DISPLAY_CHANNEL.send(AppEvent::Message(buf, len)).await;
                    }
                }
                Err(e) => {
                    error!("<< read failed {:?}", e);
                }
            }
        }
    }
}

pub trait CommandResultTrait: Sized {
    fn parse(buf: &[u8]) -> Result<Self, ()>;
}

pub struct GetDevEuiResult(pub String<16>);

impl CommandResultTrait for GetDevEuiResult {
    fn parse(buf: &[u8]) -> Result<GetDevEuiResult, ()> {
        let mut s: String<256> = String::new();
        let data = unsafe { core::str::from_utf8_unchecked(&buf) };
        if let Some(len) = data.find("+DEVEUI:") {
            if len + 32 < data.len() {
                let _ = s.push_str(&data[len + 9..len + 32]);
                while let Some(idx) = s.find(" ") {
                    s.remove(idx);
                }
                let deveui: String<16> = String::from_str(s.as_str())
                    .unwrap_or(String::<16>::from_str("0000000000000000").unwrap());
                return Ok(GetDevEuiResult(deveui));
            }
        }
        Err(())
    }
}

/// +DEVADDR: BE75D8B7
pub struct GetDevAddrResult(pub String<8>);

impl CommandResultTrait for GetDevAddrResult {
    fn parse(buf: &[u8]) -> Result<GetDevAddrResult, ()> {
        let mut s: String<8> = String::new();
        let data = unsafe { core::str::from_utf8_unchecked(&buf) };
        if let Some(len) = data.find("+DEVADDR:") {
            if len + 18 < data.len() {
                let _ = s.push_str(&data[len + 10..len + 18]);
                return Ok(GetDevAddrResult(s));
            }
        }
        Err(())
    }
}

/// +APPEUI: 00 95 69 06 00 01 28 9C
pub struct GetAppEuiResult(pub String<16>);

impl CommandResultTrait for GetAppEuiResult {
    fn parse(buf: &[u8]) -> Result<GetAppEuiResult, ()> {
        let mut s: String<256> = String::new();
        let data = unsafe { core::str::from_utf8_unchecked(&buf) };
        if let Some(len) = data.find("+APPEUI:") {
            if len + 32 < data.len() {
                let _ = s.push_str(&data[len + 9..len + 32]);
                while let Some(idx) = s.find(" ") {
                    s.remove(idx);
                }
                let deveui: String<16> = String::from_str(s.as_str())
                    .unwrap_or(String::<16>::from_str("0000000000000000").unwrap());
                return Ok(GetAppEuiResult(deveui));
            }
        }
        Err(())
    }
}

pub struct GetVerResult(pub String<128>);

impl CommandResultTrait for GetVerResult {
    fn parse(buf: &[u8]) -> Result<GetVerResult, ()> {
        let mut s: String<128> = String::new();
        let data = unsafe { core::str::from_utf8_unchecked(&buf) };
        if let Some(len) = data.find("+VER:") {
            let _ = s.push_str(&data[len + 5..(buf.len() - 6)]);
            return Ok(GetVerResult(s));
        }
        Err(())
    }
}

pub struct VoidResult();

impl CommandResultTrait for VoidResult {
    fn parse(_buf: &[u8]) -> Result<VoidResult, ()> {
        Ok(VoidResult())
    }
}

pub async fn send_command<T>(command: Command, timeout: Duration) -> Result<T, ()>
where
    T: CommandResultTrait,
{
    info!(
        "[send command]: {:?}",
        core::str::from_utf8(&command.as_bytes()).unwrap()
    );
    CHANNEL.clear();
    {
        let mut sender = SERIAL_SENDER_CHANNEL.lock().await;
        *sender = Some(CHANNEL.sender());
    }
    let _ = uart1_write(command.as_bytes().as_slice()).await;
    let rs = select(
        async {
            Timer::after(timeout).await;
            Err::<T, ()>(())
        },
        async {
            let receiver = CHANNEL.receiver();
            loop {
                let (buf, len) = receiver.receive().await;
                let data = T::parse(&buf[..len]);
                if data.is_ok() {
                    return Ok::<T, ()>(data.unwrap());
                }
            }
        },
    )
    .await;
    {
        let mut sender = SERIAL_SENDER_CHANNEL.lock().await;
        *sender = None;
    }
    match rs {
        Either::First(_) => Err(()),
        Either::Second(data) => data,
    }
}

pub async fn rx_listen() -> ([u8; 1024], usize) {
    CHANNEL.clear();
    {
        let mut sender = SERIAL_SENDER_CHANNEL.lock().await;
        *sender = Some(CHANNEL.sender());
    }
    let receiver = CHANNEL.receiver();
    receiver.receive().await
}
