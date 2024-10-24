use core::str::FromStr;

use crate::fmt::*;
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
use heapless::String;

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
    SetAppkey,
    SetAppEui,
    SetDevAddr,
    SetAppSKey,
    SetNwkSKey,
}

impl Command {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Command::GetDevEui => b"at+deveui?\r\n",
            Command::GetVer => b"at+ver?\r\n",
            Command::GetDevAddr => b"at+devaddr?\r\n",
            Command::GetAppEui => b"at+appeui?\r\n",
            Command::SetAppkey => b"set appkey\r\n",
            Command::SetAppEui => b"set app-eui\r\n",
            Command::SetDevAddr => b"set app-eui\r\n",
            Command::SetAppSKey => b"set app-eui\r\n",
            Command::SetNwkSKey => b"set app-eui\r\n",
        }
    }
}

pub async fn init(uart: Uart<'static, Async>) {
    let (tx, rx) = uart.split();
    *UART1_WRITE.lock().await = Some(tx);
    *UART1_READ.lock().await = Some(rx);
}

pub async fn uart1_write(data: &[u8]) -> Result<(), ()> {
    info!("uart write1");
    let mut lock = UART1_WRITE.lock().await;
    if lock.as_mut().unwrap().write(data).await.is_ok() {
        info!("uart write ok");
    } else {
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
            info!("uart listen");
            match uart.read_until_idle(&mut buf).await {
                Ok(len) => {
                    let mut sender = SERIAL_SENDER_CHANNEL.lock().await;
                    if let Some(sender) = sender.as_mut() {
                        sender.send((buf, len)).await;
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

pub async fn send_command<T>(command: Command, timeout: Duration) -> Result<T, ()>
where
    T: CommandResultTrait,
{
    CHANNEL.clear();
    {
        let mut sender = SERIAL_SENDER_CHANNEL.lock().await;
        *sender = Some(CHANNEL.sender());
    }
    let _ = uart1_write(command.as_bytes()).await;
    let rs = select(
        async {
            Timer::after(timeout).await;
            Err::<T, ()>(())
        },
        async {
            let receiver = CHANNEL.receiver();
            loop {
                let (buf, len) = receiver.receive().await;
                info!(
                    ">>> receiver {}",
                    core::str::from_utf8(&buf[..len]).unwrap()
                );
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
