#![no_std]
#![no_main]

mod activity;
mod config;
mod fmt;
mod lorawan;
mod serial;
mod utils;

use activity::{dislay_init, light::LightActivity, App, AppActivity};
use embassy_executor::Spawner;
use embassy_futures::select;
use embassy_stm32::{
    bind_interrupts,
    exti::ExtiInput,
    gpio::{Input, Level, Output, Pull, Speed},
    i2c,
    mode::Async,
    peripherals::{self, FLASH, PA0, PA5, PD11, PD12},
    rcc::{self, Pll},
    time::Hertz,
    usart::{self, Uart},
    Config,
};
use embassy_time::{Duration, Timer};
use fmt::*;
use lorawan::{init_lorawan_info, into_lorawan_mode};
#[cfg(not(feature = "defmt"))]
use panic_halt as _;
#[cfg(feature = "defmt")]
use {defmt_rtt as _, panic_probe as _};

use embassy_sync::{
    blocking_mutex::raw::ThreadModeRawMutex,
    channel::{Channel, Sender},
    mutex::Mutex,
};

use ssd1306::{prelude::I2CInterface, size::DisplaySize128x64, Ssd1306Async};

bind_interrupts!(struct Irqs {
    I2C1_EV => i2c::EventInterruptHandler<peripherals::I2C1>;
    I2C1_ER => i2c::ErrorInterruptHandler<peripherals::I2C1>;
    LPUART1 => usart::InterruptHandler<peripherals::LPUART1>;
});

type KeyType = Mutex<ThreadModeRawMutex, Option<ExtiInput<'static>>>;
type InputType = Mutex<ThreadModeRawMutex, Option<Input<'static>>>;
type Ssd1306DisplayType = Ssd1306Async<
    I2CInterface<i2c::I2c<'static, Async>>,
    DisplaySize128x64,
    ssd1306::mode::BufferedGraphicsModeAsync<DisplaySize128x64>,
>;

type KeyEventChannelType = Channel<ThreadModeRawMutex, AppEvent, 1>;
static DISPLAY_CHANNEL: KeyEventChannelType = Channel::new();
static KEY1: KeyType = Mutex::new(None);
static KEY2: KeyType = Mutex::new(None);
static KEY3: KeyType = Mutex::new(None);
static MODE: Mutex<ThreadModeRawMutex, Option<Output<'static>>> = Mutex::new(None);
static WAKE: Mutex<ThreadModeRawMutex, Option<Output<'static>>> = Mutex::new(None);
static BUSY: InputType = Mutex::new(None);
static STAT: InputType = Mutex::new(None);
static FLASH: Mutex<ThreadModeRawMutex, Option<FLASH>> = Mutex::new(None);
pub enum AppEvent {
    Prev,
    Next,
    Confirm,
    Back,
    NavigateTo(AppActivity),
    Message([u8; 1024], usize),
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let mut config = Config::default();
    config.rcc.hsi = true;
    config.rcc.sys = embassy_stm32::rcc::Sysclk::PLL1_R;
    config.rcc.ahb_pre = rcc::AHBPrescaler::DIV1;
    config.rcc.apb1_pre = rcc::APBPrescaler::DIV4;
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
    {
        let flash = p.FLASH;
        *FLASH.lock().await = Some(flash);
    }
    let (_oled_dc, _oled_rst) = display_pre_init(&mut p.PD11, &mut p.PD12);
    init_mode_wake(p.PA0, p.PA5).await;
    {
        let busy = Input::new(p.PA6, Pull::Up);
        info!("busy {:?}", busy.is_low());
        *BUSY.lock().await = Some(busy);
        let state = Input::new(p.PA7, Pull::Up);
        info!("state {:?}", state.is_low());
        *STAT.lock().await = Some(state);
        let button = ExtiInput::new(p.PD8, p.EXTI8, Pull::Up);
        *KEY1.lock().await = Some(button);
        let button = ExtiInput::new(p.PD10, p.EXTI10, Pull::Up);
        *KEY2.lock().await = Some(button);
        let button = ExtiInput::new(p.PD9, p.EXTI9, Pull::Up);
        *KEY3.lock().await = Some(button);
    }
    let mut config = usart::Config::default();
    config.baudrate = 9600;
    let uart = Uart::new(
        p.LPUART1, p.PC0, p.PC1, Irqs, p.DMA2_CH6, p.DMA2_CH7, config,
    );
    if let Ok(uart) = uart {
        serial::init(uart).await;
    } else {
        info!("uart init failed {:?}", uart.err());
    }
    let i2c = init_display_i2c!(p);
    unwrap!(spawner.spawn(serial::serial_listen()));
    unwrap!(spawner.spawn(key_handle()));
    let _ = init_lorawan_info().await;
    unwrap!(spawner.spawn(join_network()));
    dislay_init(i2c).await;
    let _ = config::init().await;
    let app = App::new();
    app.show().await;
}

#[embassy_executor::task]
async fn key_handle() {
    let mut key1 = KEY1.lock().await;
    let key1 = key1.as_mut().unwrap();

    let mut key2 = KEY2.lock().await;
    let key2 = key2.as_mut().unwrap();

    let mut key3 = KEY3.lock().await;
    let key3 = key3.as_mut().unwrap();
    loop {
        select::select3(
            async {
                key1.wait_for_any_edge().await;
                if !key1.is_high() {
                    DISPLAY_CHANNEL.send(AppEvent::Next).await;
                }
            },
            async {
                key2.wait_for_any_edge().await;
                if !key2.is_high() {
                    DISPLAY_CHANNEL.send(AppEvent::Prev).await;
                }
            },
            async {
                key3.wait_for_any_edge().await;
                if !key3.is_high() {
                    DISPLAY_CHANNEL.send(AppEvent::Confirm).await;
                }
            },
        )
        .await;
    }
}

#[embassy_executor::task]
async fn join_network() {
    loop {
        if into_lorawan_mode().await {
            DISPLAY_CHANNEL
                .send(AppEvent::NavigateTo(AppActivity::Light(
                    LightActivity::new(),
                )))
                .await;
            
            break;
        } else {
            info!("join failed");
            Timer::after(Duration::from_secs(5)).await;
        }
    }
}

//async fn register()=>{}


fn display_pre_init<'a>(pd11: &'a mut PD11, pd12: &'a mut PD12) -> (Output<'a>, Output<'a>) {
    let mut oled_dc = Output::new(pd11, Level::Low, Speed::Low);
    let mut oled_rst = Output::new(pd12, Level::Low, Speed::Low);
    oled_dc.set_low();
    oled_rst.set_high();
    oled_rst.set_low();
    oled_rst.set_high();
    return (oled_dc, oled_rst);
}

async fn init_mode_wake(pa0: PA0, pa5: PA5) {
    let mut mode = Output::new(pa0, Level::Low, Speed::Low);
    let mut wake = Output::new(pa5, Level::Low, Speed::Low);
    mode.set_high();
    wake.set_high();
    *MODE.lock().await = Some(mode);
    *WAKE.lock().await = Some(wake);
}

pub async fn into_cmd_mode() {
    let mut mode = MODE.lock().await;
    let mode = mode.as_mut().unwrap();
    mode.set_high();
    Timer::after(Duration::from_millis(300)).await;
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
