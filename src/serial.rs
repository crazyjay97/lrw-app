use crate::{fmt::*, Irqs};
use embassy_stm32::{
    mode::Async,
    usart::{self, Uart, UartRx, UartTx},
};
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};

enum Command {
    GetDevEui,
    GetVer,
    SetAppkey,
    SetAppEui,
    SetDevAddr,
    SetAppSKey,
    SetNwkSKey,
}

type UART1ReadType = Mutex<ThreadModeRawMutex, Option<UartRx<'static, Async>>>;
type UART1WriteType = Mutex<ThreadModeRawMutex, Option<UartTx<'static, Async>>>;

static UART1_READ: UART1ReadType = Mutex::new(None);
static UART1_WRITE: UART1WriteType = Mutex::new(None);

pub async fn init(uart: Uart<'static, Async>) {
    let (tx, rx) = uart.split();
    *UART1_WRITE.lock().await = Some(tx);
    *UART1_READ.lock().await = Some(rx);
}

async fn uart1_write(data: &[u8]) -> Result<(), ()> {
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
                Ok(len) => {}
                Err(e) => {
                    error!("<< read failed {:?}", e);
                }
            }
        }
    }
}

trait CommandResultTrait {}

struct GetDevEuiResult<'a>(&'a str);

impl<'a> CommandResultTrait for GetDevEuiResult<'a> {}

// pub async fn send_command<T>(command: Command) -> Result<T, ()> where T: CommandResultTrait {
//     match command {
//         Command::GetDevEui => {
//             return Ok(GetDevEuiResult("0000000000000000"));
//         },
//         _ => {
//             return Ok(GetDevEuiResult("0000000000000000"));
//         }
//     }
// }
