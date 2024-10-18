#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_stm32::{
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Level, Output, Pull, Speed},
    i2c,
    mode::Async,
    peripherals::{self, PD11, PD12},
    rcc::{self, Pll},
    time::Hertz,
    usart::{self, Uart, UartRx, UartTx},
    Config,
};

use embassy_sync::{
    blocking_mutex::raw::ThreadModeRawMutex,
    channel::{Channel, Sender},
    mutex::Mutex,
};
use embassy_time::Duration;
use embedded_graphics::{
    image::Image,
    pixelcolor::{self, BinaryColor},
    prelude::*,
};
use heapless::Vec;
use panic_probe as _;
use ssd1306::{prelude::*, size::DisplaySize128x64, I2CDisplayInterface, Ssd1306Async};
use tinybmp::Bmp;

bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
    LPUART1 => usart::InterruptHandler<peripherals::LPUART1>;
});

type UART1ReadType = Mutex<ThreadModeRawMutex, Option<UartRx<'static, Async>>>;
type UART1WriteType = Mutex<ThreadModeRawMutex, Option<UartTx<'static, Async>>>;
type Key1Type = Mutex<ThreadModeRawMutex, Option<ExtiInput<'static>>>;
type Ssd1306DisplayType = Ssd1306Async<
    I2CInterface<i2c::I2c<'static, Async>>,
    DisplaySize128x64,
    ssd1306::mode::BufferedGraphicsModeAsync<DisplaySize128x64>,
>;
type DisplayType = Mutex<ThreadModeRawMutex, Option<Ssd1306DisplayType>>;

static UART1_READ: UART1ReadType = Mutex::new(None);
static UART1_WRITE: UART1WriteType = Mutex::new(None);
static KEY1: Key1Type = Mutex::new(None);
static DISPLAY_CHANNEL: Channel<ThreadModeRawMutex, (Vec<u8, 1024>, usize, bool), 2> =
    Channel::new();

static DISPLAY: DisplayType = Mutex::new(None);

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.hsi = true;
    config.rcc.sys = embassy_stm32::rcc::Sysclk::PLL1_R;
    config.rcc.ahb_pre = rcc::AHBPrescaler::DIV1;
    config.rcc.apb1_pre = rcc::APBPrescaler::DIV1;
    config.rcc.apb2_pre = rcc::APBPrescaler::DIV1;
    config.rcc.pll = Some(Pll {
        source: rcc::PllSource::MSI,
        prediv: rcc::PllPreDiv::DIV1,
        mul: rcc::PllMul::MUL32,
        divp: None,
        divq: Some(rcc::PllQDiv::DIV2),
        divr: Some(rcc::PllRDiv::DIV2),
    });
    config.rcc.mux.lptim1sel = rcc::mux::Lptim1sel::PCLK1;
    config.rcc.mux.i2c1sel = rcc::mux::I2c1sel::SYS;
    let mut p = embassy_stm32::init(config);
    let (_oled_dc, _oled_rst) = display_pre_init(&mut p.PD11, &mut p.PD12);

    let button = ExtiInput::new(p.PD8, p.EXTI8, Pull::Up);
    {
        *KEY1.lock().await = Some(button);
    }
    {
        let mut config = usart::Config::default();
        config.baudrate = 9600;
        let uart = Uart::new(
            p.LPUART1, p.PC0, p.PC1, Irqs, p.DMA2_CH6, p.DMA2_CH7, config,
        );
        if let Ok(uart) = uart {
            let (tx, rx) = uart.split();
            *UART1_WRITE.lock().await = Some(tx);
            *UART1_READ.lock().await = Some(rx);
        } else {
            info!("uart init failed {:?}", uart.err());
        }
    }
    let i2c = init_display_i2c!(p);
    //unwrap!(spawner.spawn(serial_listen(DISPLAY_CHANNEL.sender())));
    unwrap!(spawner.spawn(key1_handle(DISPLAY_CHANNEL.sender())));
    dislay_init(i2c).await;
}

#[embassy_executor::task]
async fn key1_handle(sender: Sender<'static, ThreadModeRawMutex, (Vec<u8, 1024>, usize, bool), 2>) {
    let mut button = KEY1.lock().await.take().unwrap();
    loop {
        button.wait_for_any_edge().await;
        if button.is_high() {
            info!("button pressed");
        } else {
            info!("button released");
            //let cmd = "at+deveui?\r\n".as_bytes();
            //let _ = uart1_write(cmd).await;
            let buf = [0u8; 1024];
            let b: Vec<u8, 1024> = Vec::from_slice(&buf).unwrap();
            {
                sender.send((b, 0, true)).await;
            }
        }
    }
}

#[inline]
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
async fn serial_listen(
    sender: Sender<'static, ThreadModeRawMutex, (Vec<u8, 1024>, usize, bool), 2>,
) {
    loop {
        let mut buf = [0u8; 1024];
        let mut lock = UART1_READ.lock().await;
        if let Some(uart) = lock.as_mut() {
            info!("uart listen");
            match uart.read_until_idle(&mut buf).await {
                Ok(len) => {
                    let b: Vec<u8, 1024> = Vec::from_slice(&mut buf).unwrap();
                    //sender.send((b, len, false)).await;
                }
                Err(e) => {
                    error!("<<<<<<<<<<< read failed {:?}", e);
                }
            }
        }
    }
}

fn display_pre_init<'a>(pd11: &'a mut PD11, pd12: &'a mut PD12) -> (Output<'a>, Output<'a>) {
    let mut oled_dc = Output::new(pd11, Level::Low, Speed::Low);
    let mut oled_rst = Output::new(pd12, Level::Low, Speed::Low);
    oled_dc.set_low();
    oled_rst.set_high();
    oled_rst.set_low();
    oled_rst.set_high();
    return (oled_dc, oled_rst);
}

#[macro_export]
macro_rules! init_display_i2c {
    ($p:expr) => {{
        let i2c = embassy_stm32::i2c::I2c::new(
            $p.I2C1,
            $p.PB8,
            $p.PB9,
            Irqs,
            $p.DMA1_CH6,
            $p.DMA1_CH7,
            Hertz::mhz(15),
            Default::default(),
        );
        (i2c)
    }};
}

async fn dislay_init(i2c: embassy_stm32::i2c::I2c<'static, Async>) {
    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306Async::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display.init().await.unwrap();
    let _ = display.flush().await;
    {
        *DISPLAY.lock().await = Some(display);
    }
    let mut idx = 0;
    draw_menus(idx, true).await;
    loop {
        let message: (Vec<u8, 1024>, usize, bool) = DISPLAY_CHANNEL.receive().await;
        match core::str::from_utf8(&message.0[0..message.1]) {
            Ok(data) => {
                if message.2 {
                    idx += 1;
                    draw_menus(idx, true).await;
                } else {
                    idx -= 1;
                    draw_menus(idx, false).await;
                }
            }
            Err(_) => {
                info!("utf8 parse failed");
            }
        }
    }
}

struct Menu {
    bmp: Bmp<'static, BinaryColor>,
    label: &'static str,
}

/// 单页菜单
#[inline]
async fn draw_menus<'a>(idx: usize, to_left: bool) {
    // 菜单列表
    let menus = [
        Menu {
            bmp: load_bmp(include_bytes!("../assets/info.bmp")).unwrap(),
            label: "info",
        },
        Menu {
            bmp: load_bmp(include_bytes!("../assets/app.bmp")).unwrap(),
            label: "console",
        },
        Menu {
            bmp: load_bmp(include_bytes!("../assets/info.bmp")).unwrap(),
            label: "debug",
        },
        Menu {
            bmp: load_bmp(include_bytes!("../assets/app.bmp")).unwrap(),
            label: "sos",
        },
        Menu {
            bmp: load_bmp(include_bytes!("../assets/info.bmp")).unwrap(),
            label: "find my",
        },
        Menu {
            bmp: load_bmp(include_bytes!("../assets/app.bmp")).unwrap(),
            label: "app store",
        },
        Menu {
            bmp: load_bmp(include_bytes!("../assets/info.bmp")).unwrap(),
            label: "imessage",
        },
    ];
    let mut display = DISPLAY.lock().await;
    let display = display.as_mut().unwrap();
    let menu = &menus[idx];
    let pos = calc_start_pos((menu.bmp.size().width as i32, menu.bmp.size().height as i32));
    let w = DisplaySize128x64::WIDTH as i32;
    let mut offset_x = w + pos.0;
    info!("draw menu {} {} {}", offset_x, w, pos.0);
    let mut last: Option<(i32, i32, &Menu)> = {
        if to_left {
            if idx > 0 {
                let menu = &menus[idx - 1];
                let pos =
                    calc_start_pos((menu.bmp.size().width as i32, menu.bmp.size().height as i32));
                Some((pos.0, pos.1, menu))
            } else {
                None
            }
        } else {
            if idx < menus.len() - 1 {
                let menu = &menus[idx - 1];
                let pos =
                    calc_start_pos((menu.bmp.size().width as i32, menu.bmp.size().height as i32));
                Some((pos.0, pos.1, menu))
            } else {
                None
            }
        }
    };
    loop {
        let _ = display.clear(BinaryColor::Off);
        //info!("draw menu {} {} {}", offset_x, w, pos.0);
        // move last
        if let Some(last) = last.as_mut() {
            draw_menu(display, &last.2.bmp, (last.0, last.1)).await;
            last.0 = if to_left { last.0 + 4 } else { last.0 - 4 }
        }
        // draw current menu
        if offset_x <= w {
            draw_menu(display, &menu.bmp, (offset_x, pos.1)).await;
        }
        if offset_x == pos.0 {
            info!("stop!!!!!!");
            break;
        }
        let _ = display.flush().await;
        offset_x = if to_left { offset_x.0 + 4 } else { offset_x.0 - 4 }
    }
}

/// 图像需要居中,计算图像开始位置,图像居中时左上角的位置
fn calc_start_pos(size: (i32, i32)) -> (i32, i32) {
    let h = DisplaySize128x64::HEIGHT;
    let w = DisplaySize128x64::WIDTH;
    let x = (w as i32 - size.0 as i32) / 2;
    let y = (h as i32 - size.1 as i32) / 2;
    return (x, y);
}

#[inline]
async fn draw_menu<'a>(
    display: &mut Ssd1306DisplayType,
    bmp: &Bmp<'a, BinaryColor>,
    pos: (i32, i32),
) {
    let image = Image::new(bmp, Point::new(pos.0, pos.1));
    let _ = image.draw(display);
}

fn load_bmp<'a>(slice: &'a [u8]) -> Result<Bmp<'a, BinaryColor>, ()> {
    let bmp: Result<Bmp<BinaryColor>, tinybmp::ParseError> = Bmp::from_slice(&slice);
    match bmp {
        Ok(bmp) => {
            info!("bmp parse ok");
            return Ok(bmp);
        }
        Err(e) => match e {
            tinybmp::ParseError::UnsupportedBpp(_) => {
                info!("bmp parse failed:UnsupportedBpp");
            }
            tinybmp::ParseError::UnexpectedEndOfFile => {
                info!("bmp parse failed:UnexpectedEndOfFile");
            }
            tinybmp::ParseError::InvalidFileSignature(b) => {
                info!("bmp parse failed:InvalidFileSignature {:02X}", b);
            }
            tinybmp::ParseError::UnsupportedCompressionMethod(_) => {
                info!("bmp parse failed:UnsupportedCompressionMethod");
            }
            tinybmp::ParseError::UnsupportedHeaderLength(_) => {
                info!("bmp parse failed:UnsupportedHeaderLength");
            }
            tinybmp::ParseError::UnsupportedChannelMasks => {
                info!("bmp parse failed:UnsupportedChannelMasks");
            }
            tinybmp::ParseError::InvalidImageDimensions => {
                info!("bmp parse failed:InvalidImageDimensions");
            }
        },
    }
    Err(())
}
