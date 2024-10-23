use core::borrow::Borrow;

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
                    info!("===================lock");
                    let mut sender = SERIAL_SENDER_CHANNEL.lock().await;
                    info!("===================unlock");
                    if let Some(sender) = sender.as_mut() {
                        info!("===================send");
                        sender.send((buf, len)).await;
                    }
                    info!("===================fn");
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

pub struct GetDevEuiResult(pub String<256>);

impl CommandResultTrait for GetDevEuiResult {
    fn parse(buf: &[u8]) -> Result<GetDevEuiResult, ()> {
        let mut s: String<256> = String::new();
        let data = unsafe { core::str::from_utf8_unchecked(&buf) };
        info!(">>> {}", data);
        if let Some(len) = data.find("+DEVEUI:") {
            info!(">>> LEN: {} IDX: {}", data.len(), len);
            if len + 8 + 32 < data.len() {
                let _ = s.push_str(&data[len + 8..len + 8 + 32]);
                return Ok(GetDevEuiResult(s));
            }
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
            info!(">>> select");
            let receiver = CHANNEL.receiver();
            loop {
                let (buf, len) = receiver.receive().await;
                info!(">>> receiver");
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
