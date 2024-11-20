use core::cell::RefCell;

mod device_info;
mod factory;
pub mod light;
use crate::lorawan::{Joined, LORAWAN_STATE};
use crate::{fmt::*, lorawan};
use crate::{
    utils::{
        self,
        qrcode::{QrCodeEcc, Version},
    },
    AppEvent, Ssd1306DisplayType, DISPLAY_CHANNEL,
};
use core::fmt::Write;
use device_info::DeviceInfoActivity;
use embassy_futures::select::{select, select3};
use embassy_stm32::i2c::I2c;
use embassy_stm32::mode::Async;
use embassy_sync::channel::Channel;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use embedded_graphics::mono_font::iso_8859_7::FONT_10X20;
use embedded_graphics::{
    image::Image,
    mono_font::{iso_8859_5::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use factory::FactoryActivity;
use heapless::String;
use light::LightActivity;
use ssd1306::{prelude::*, size::DisplaySize128x64, I2CDisplayInterface, Ssd1306Async};
use tinybmp::Bmp;

type DisplayType = Mutex<ThreadModeRawMutex, Option<Ssd1306DisplayType>>;
type ViewChanType = Channel<ThreadModeRawMutex, u8, 1>;

static DISPLAY: DisplayType = Mutex::new(None);

pub struct App {
    current_activity: RefCell<AppActivity>,
    next_activity: RefCell<Option<AppActivity>>,
    chan: ViewChanType,
}

impl App {
    pub fn new() -> Self {
        let chan = Channel::new();
        Self {
            current_activity: RefCell::new(AppActivity::EuiQrCode(EuiQrCodeActivity::new())),
            next_activity: RefCell::new(None),
            chan,
        }
    }

    pub async fn show(&self) {
        loop {
            select3(
                async {
                    loop {
                        let event: AppEvent = DISPLAY_CHANNEL.receive().await;
                        self.key_handle(event).await;
                    }
                },
                async {
                    let _ = self.chan.receive().await;
                },
                async {
                    {
                        match &*self.current_activity.borrow() {
                            AppActivity::Main(ref main_activity) => {
                                main_activity.show().await;
                            }
                            AppActivity::EuiQrCode(ref eui_qr_code_activity) => {
                                eui_qr_code_activity.show().await;
                            }
                            AppActivity::DeviceInfo(device_info_activity) => {
                                device_info_activity.show().await;
                            }
                            AppActivity::Factory(factory_activity) => {
                                factory_activity.show().await;
                            }
                            AppActivity::Todo(todo_activity) => {
                                todo_activity.show().await;
                            }
                            AppActivity::Light(light_activity) => {
                                light_activity.show().await;
                            }
                        }
                    }
                    loop {
                        Timer::after(Duration::from_secs(1)).await;
                    }
                },
            )
            .await;
        }
    }

    pub async fn key_handle(&self, e: AppEvent) {
        {
            match &*self.current_activity.borrow() {
                AppActivity::Main(ref main_activity) => {
                    main_activity.key_handle(e, &self).await;
                }
                AppActivity::EuiQrCode(ref eui_qr_code_activity) => {
                    eui_qr_code_activity.key_handle(e, &self).await;
                }
                AppActivity::DeviceInfo(device_info_activity) => {
                    device_info_activity.key_handle(e, &self).await;
                }
                AppActivity::Factory(factory_activity) => {
                    factory_activity.key_handle(e, &self).await;
                }
                AppActivity::Todo(todo_activity) => {
                    todo_activity.key_handle(e, &self).await;
                }
                AppActivity::Light(light_activity) => {
                    light_activity.key_handle(e, &self).await;
                }
            }
        }
        if let Some(next_activity) = self.next_activity.take() {
            {
                let c = self.current_activity.try_borrow_mut();
                if let Ok(mut c) = c {
                    *c = next_activity;
                } else {
                    info!("err {:?}", c.err());
                }
            }
            self.chan.send(0).await;
        }
    }

    async fn navigate_to(&self, activity: AppActivity) {
        *self.next_activity.borrow_mut() = Some(activity);
    }
}

pub enum AppActivity {
    Main(MainActivity),
    EuiQrCode(EuiQrCodeActivity),
    DeviceInfo(DeviceInfoActivity),
    Factory(FactoryActivity),
    Light(LightActivity),
    Todo(TodoActivity),
}

trait Activity {
    async fn key_handle(&self, e: AppEvent, app: &App);
    async fn show(&self);
}

pub struct MainActivity {
    menus: [Menu; Self::MENU_LEN],
    menu_index: RefCell<usize>,
}

impl Activity for MainActivity {
    async fn key_handle(&self, e: AppEvent, app: &App) {
        match e {
            AppEvent::Prev => self.draw_menus(1).await,
            AppEvent::Next => self.draw_menus(-1).await,
            AppEvent::Confirm => {
                let idx = self.menu_index.borrow();
                let menu = &self.menus[*idx];
                match menu.label {
                    "info" => {
                        app.navigate_to(AppActivity::DeviceInfo(DeviceInfoActivity::new()))
                            .await;
                    }
                    "device code" => {
                        app.navigate_to(AppActivity::EuiQrCode(EuiQrCodeActivity::new()))
                            .await;
                    }
                    "factory" => {
                        app.navigate_to(AppActivity::Factory(FactoryActivity::new()))
                            .await;
                    }
                    "app" => {
                        app.navigate_to(AppActivity::Light(LightActivity::new()))
                            .await;
                    }
                    _ => {
                        app.navigate_to(AppActivity::Todo(TodoActivity::new()))
                            .await;
                    }
                }
            }
            AppEvent::Back => {}
            AppEvent::Message(_, _) => {}
            _ => {}
        }
    }

    async fn show(&self) {
        self.draw_menus(0).await;
    }
}

impl MainActivity {
    const MENU_LEN: usize = 4;
    fn new() -> Self {
        let menus: [Menu; Self::MENU_LEN] = [
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/info.bmp")).unwrap(),
                label: "info",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/app.bmp")).unwrap(),
                label: "app",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/qrcode.bmp")).unwrap(),
                label: "device code",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/factory.bmp")).unwrap(),
                label: "factory",
            },
        ];
        Self {
            menus: menus,
            menu_index: RefCell::new(0),
        }
    }
    async fn draw_menus<'a>(&self, dire: i8) {
        let mut current_idx = self.menu_index.borrow_mut();
        let menus: &[Menu; Self::MENU_LEN] = &self.menus;
        if (dire == 1 && *current_idx >= (menus.len() - 1)) || (dire == -1 && *current_idx == 0) {
            return;
        }
        let idx: usize = if dire == 1 {
            *current_idx += 1;
            *current_idx
        } else if dire == -1 {
            *current_idx -= 1;
            *current_idx
        } else {
            *current_idx
        };
        const STEP: i32 = 10;
        let mut display = DISPLAY.lock().await;
        let display = display.as_mut().unwrap();
        let menu = &menus[idx];
        let pos = calc_start_pos((menu.bmp.size().width as i32, menu.bmp.size().height as i32));
        let w = DisplaySize128x64::WIDTH as i32;
        let mut offset_x = if dire >= 0 { w + pos.0 } else { pos.0 - w };
        let mut last: Option<(i32, i32, &Menu)> = {
            if dire >= 0 {
                if idx > 0 {
                    let menu = &menus[idx - 1];
                    let pos = calc_start_pos((
                        menu.bmp.size().width as i32,
                        menu.bmp.size().height as i32,
                    ));
                    Some((pos.0, pos.1, menu))
                } else {
                    None
                }
            } else {
                if idx <= menus.len() - 2 {
                    let menu = &menus[idx + 1];
                    let pos = calc_start_pos((
                        menu.bmp.size().width as i32,
                        menu.bmp.size().height as i32,
                    ));
                    Some((pos.0, pos.1, menu))
                } else {
                    None
                }
            }
        };
        let character_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(BinaryColor::On)
            .build();
        let center_aligned = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Bottom)
            .build();
        loop {
            let _ = display.clear(BinaryColor::Off);
            // move last
            if let Some(last) = last.as_mut() {
                self.draw_menu(display, &last.2.bmp, (last.0, last.1)).await;
                last.0 = if dire >= 0 {
                    last.0 - STEP
                } else {
                    last.0 + STEP
                }
            }
            // draw current menu
            if dire >= 0 {
                if offset_x <= w {
                    self.draw_menu(display, &menu.bmp, (offset_x, pos.1)).await;
                }
                if offset_x == pos.0 {
                    let _ = Text::with_text_style(
                        menu.label,
                        Point { x: 64, y: 64 },
                        character_style,
                        center_aligned,
                    )
                    .draw(display);
                    let _ = display.flush().await;
                    break;
                }
            } else {
                if offset_x <= w {
                    self.draw_menu(display, &menu.bmp, (offset_x, pos.1)).await;
                }
                if offset_x == pos.0 {
                    let _ = Text::with_text_style(
                        menu.label,
                        Point { x: 64, y: 64 },
                        character_style,
                        center_aligned,
                    )
                    .draw(display);
                    let _ = display.flush().await;
                    break;
                }
            }
            let _ = display.flush().await;
            offset_x = if dire >= 0 {
                if offset_x - STEP < pos.0 {
                    offset_x - (offset_x - pos.0)
                } else {
                    offset_x - STEP
                }
            } else {
                if offset_x + STEP > pos.0 {
                    offset_x + (pos.0 - offset_x)
                } else {
                    offset_x + STEP
                }
            }
        }
    }

    #[inline]
    async fn draw_menu<'a>(
        &self,
        display: &mut Ssd1306DisplayType,
        bmp: &Bmp<'a, BinaryColor>,
        pos: (i32, i32),
    ) {
        let image = Image::new(bmp, Point::new(pos.0, pos.1));
        let _ = image.draw(display);
    }
}

struct Menu {
    bmp: Bmp<'static, BinaryColor>,
    label: &'static str,
}

/// 单页菜单
#[inline]

/// 图像需要居中,计算图像开始位置,图像居中时左上角的位置
fn calc_start_pos(size: (i32, i32)) -> (i32, i32) {
    let w = DisplaySize128x64::WIDTH;
    let x = (w as i32 - size.0 as i32) / 2;
    return (x, 5);
}

pub async fn dislay_init(i2c: I2c<'static, Async>) {
    let interface = I2CDisplayInterface::new(i2c);
    let mut display = Ssd1306Async::new(interface, DisplaySize128x64, DisplayRotation::Rotate0)
        .into_buffered_graphics_mode();
    display.init().await.unwrap();
    let _ = display.flush().await;
    {
        *DISPLAY.lock().await = Some(display);
    }
}

fn load_bmp<'a>(slice: &'a [u8]) -> Result<Bmp<'a, BinaryColor>, ()> {
    let bmp: Result<Bmp<BinaryColor>, tinybmp::ParseError> = Bmp::from_slice(&slice);
    match bmp {
        Ok(bmp) => {
            info!("bmp parse ok");
            return Ok(bmp);
        }
        Err(_e) => {
            error!("parse bmp failed");
        }
    }
    Err(())
}

pub struct EuiQrCodeActivity {
    chan: Channel<ThreadModeRawMutex, u8, 1>,
}

impl EuiQrCodeActivity {
    fn new() -> Self {
        Self {
            chan: Channel::new(),
        }
    }
}

impl Activity for EuiQrCodeActivity {
    async fn key_handle(&self, e: AppEvent, app: &App) {
        match e {
            AppEvent::Next => {
                //app.show().await;
            }
            AppEvent::Prev => {
                //app.show().await;
            }
            AppEvent::Confirm => {
                self.done().await;
                app.navigate_to(AppActivity::Main(MainActivity::new()))
                    .await
            }
            AppEvent::Back => {
                //app.show().await;
            }
            AppEvent::Message(buf, size) => {
                if size == 4 && buf[0] >> 7 == 1 {
                    {
                        let mut lrw_state = LORAWAN_STATE.lock().await;
                        lrw_state.2 = Joined::Yes;
                    }
                    self.refresh().await;
                }
            }
            AppEvent::Refresh => {
                self.refresh().await;
            }
            AppEvent::NavigateTo(activity) => {
                self.done().await;
                Timer::after(Duration::from_millis(300)).await;
                app.navigate_to(activity).await;
            }
        }
    }

    async fn show(&self) {
        let mut display = DISPLAY.lock().await;
        let mut display = display.as_mut().unwrap();
        let _ = display.clear(BinaryColor::Off);
        let mut outbuffer = [0u8; Version::MAX.buffer_len()];
        let mut tempbuffer = [0u8; Version::MAX.buffer_len()];
        let lorawan_info = lorawan::LORAWAN.lock().await;
        let lorawan_info = lorawan_info.as_ref().unwrap();
        let eui = lorawan_info.deveui.as_ref().unwrap();
        let qr = utils::qrcode::QrCode::encode_text(
            eui.as_str(),
            &mut tempbuffer,
            &mut outbuffer,
            QrCodeEcc::Low,
            Version::MIN,
            Version::MAX,
            None,
            true,
        )
        .unwrap();
        // 放大倍数
        let scale: u32 = 3;
        let screen_height = 64;
        let qr_size = qr.size() as u32 * scale;
        let offset_x = 0;
        let offset_y = (screen_height - qr_size as u32) / 2;
        for y in 0..qr.size() {
            for x in 0..qr.size() {
                for dy in 0..scale {
                    for dx in 0..scale {
                        display.set_pixel(
                            (x as u32) * scale + dx + offset_x,
                            (y as u32) * scale + dy + offset_y,
                            qr.get_module(x, y),
                        );
                    }
                }
            }
        }
        let _ = display.flush().await;

        loop {
            let state = { *lorawan::LORAWAN_STATE.lock().await };
            self.draw_state(&mut display, "Busy", &state.0, 128, 10)
                .await;
            self.draw_state(&mut display, "State", &state.1, 128, 30)
                .await;
            self.draw_state(&mut display, "Joined", &state.2, 128, 50)
                .await;
            let _ = display.flush().await;
            let c = self.chan.receive().await;
            if c == 0xFF {
                return;
            }
        }
    }
}

impl EuiQrCodeActivity {
    pub async fn done(&self) {
        self.chan.send(0xFF).await;
        Timer::after(Duration::from_millis(50)).await;
    }

    pub async fn refresh(&self) {
        self.chan.send(0).await;
    }

    async fn draw_state<'a, T: Into<&'a str>>(
        &self,
        display: &mut Ssd1306DisplayType,
        label: &str,
        state: T,
        x: i32,
        y: i32,
    ) {
        let style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(BinaryColor::On)
            .background_color(BinaryColor::Off)
            .build();
        let aligned = TextStyleBuilder::new()
            .alignment(Alignment::Right)
            .baseline(Baseline::Middle)
            .build();
        let mut s = String::<16>::new();
        let _ = write!(s, "{}: {}", label, state.into());
        let _ = Text::with_text_style(s.as_str(), Point::new(x, y), style, aligned).draw(display);
    }
}

pub struct TodoActivity();

impl TodoActivity {
    fn new() -> Self {
        Self()
    }
}

impl Activity for TodoActivity {
    async fn key_handle(&self, e: AppEvent, app: &App) {
        match e {
            crate::AppEvent::NavigateTo(_) => {}
            AppEvent::Next => {
                //app.show().await;
            }
            AppEvent::Prev => {
                //app.show().await;
            }
            AppEvent::Confirm => {
                app.navigate_to(AppActivity::Main(MainActivity::new()))
                    .await
            }
            AppEvent::Back => {
                //app.show().await;
            }
            AppEvent::Message(_, _) => {}
            _ => {}
        }
    }

    async fn show(&self) {
        let mut display = DISPLAY.lock().await;
        let display = display.as_mut().unwrap();
        let _ = display.clear(BinaryColor::Off);
        let character_style = MonoTextStyleBuilder::new()
            .font(&FONT_10X20)
            .text_color(BinaryColor::On)
            .build();
        let left_aligned = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();
        Text::with_text_style(
            "TODO",
            display.bounding_box().center(),
            character_style,
            left_aligned,
        )
        .draw(display)
        .unwrap_or(Point { x: 0, y: 0 });

        let _ = display.flush().await;
    }
}
