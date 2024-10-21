use core::cell::RefCell;

use crate::{
    utils::{
        self,
        qrcode::{QrCodeEcc, Version},
    },
    KeyEvent, Ssd1306DisplayType, DISPLAY_CHANNEL,
};
use crate::fmt::*;
use embassy_stm32::i2c::I2c;
use embassy_stm32::mode::Async;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use embedded_graphics::{
    image::Image,
    mono_font::{iso_8859_5::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use ssd1306::{prelude::*, size::DisplaySize128x64, I2CDisplayInterface, Ssd1306Async};
use tinybmp::Bmp;

type DisplayType = Mutex<ThreadModeRawMutex, Option<Ssd1306DisplayType>>;
static DISPLAY: DisplayType = Mutex::new(None);

struct App {
    current_activity: RefCell<AppActivity>,
    next_activity: RefCell<Option<AppActivity>>,
}

impl App {
    fn new() -> Self {
        Self {
            current_activity: RefCell::new(AppActivity::Main(MainActivity::new())),
            next_activity: RefCell::new(None),
        }
    }

    async fn show(&self) {
        match &*self.current_activity.borrow() {
            AppActivity::Main(ref main_activity) => {
                main_activity.show().await;
            }
            AppActivity::EuiQrCode(ref eui_qr_code_activity) => {
                eui_qr_code_activity.show().await;
            }
        }
    }

    async fn key_handle(&self, e: KeyEvent) {
        match &*self.current_activity.borrow() {
            AppActivity::Main(ref main_activity) => {
                main_activity.key_handle(e, &self).await;
            }
            AppActivity::EuiQrCode(ref eui_qr_code_activity) => {
                eui_qr_code_activity.key_handle(e, &self).await;
            }
        }
        if let Some(next_activity) = self.next_activity.take() {
            {
                *self.current_activity.borrow_mut() = next_activity;
            }
            self.show().await;
        }
    }

    async fn navigate_to(&self, activity: AppActivity) {
        *self.next_activity.borrow_mut() = Some(activity);
    }
}

pub enum AppActivity {
    Main(MainActivity),
    EuiQrCode(EuiQrCodeActivity),
}

trait Activity {
    async fn key_handle(&self, e: KeyEvent, app: &App);
    async fn show(&self);
}

pub struct MainActivity {
    menus: [Menu; 7],
    menu_index: RefCell<usize>,
}

impl Activity for MainActivity {
    async fn key_handle(&self, e: KeyEvent, app: &App) {
        match e {
            KeyEvent::Prev => self.draw_menus(-1).await,
            KeyEvent::Next => self.draw_menus(1).await,
            KeyEvent::Confirm => {
                app.navigate_to(AppActivity::EuiQrCode(EuiQrCodeActivity::new()))
                    .await;
            }
            KeyEvent::Back => {}
        }
    }

    async fn show(&self) {
        self.draw_menus(0).await;
    }
}

impl MainActivity {
    fn new() -> Self {
        let menus = [
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/info.bmp")).unwrap(),
                label: "info",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/app.bmp")).unwrap(),
                label: "console",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/info.bmp")).unwrap(),
                label: "debug",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/app.bmp")).unwrap(),
                label: "sos",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/info.bmp")).unwrap(),
                label: "find my",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/app.bmp")).unwrap(),
                label: "app store",
            },
            Menu {
                bmp: load_bmp(include_bytes!("../../assets/info.bmp")).unwrap(),
                label: "imessage",
            },
        ];
        Self {
            menus: menus,
            menu_index: RefCell::new(0),
        }
    }
    async fn draw_menus<'a>(&self, dire: i8) {
        let mut current_idx = self.menu_index.borrow_mut();
        let menus: &[Menu; 7] = &self.menus;
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
    let app = App::new();
    app.show().await;
    loop {
        let event: KeyEvent = DISPLAY_CHANNEL.receive().await;
        app.key_handle(event).await;
    }
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

pub struct EuiQrCodeActivity {}

impl EuiQrCodeActivity {
    fn new() -> Self {
        Self {}
    }
}

impl Activity for EuiQrCodeActivity {
    async fn key_handle(&self, e: KeyEvent, app: &App) {
        match e {
            KeyEvent::Next => {
                //app.show().await;
            }
            KeyEvent::Prev => {
                //app.show().await;
            }
            KeyEvent::Confirm => {
                app.navigate_to(AppActivity::Main(MainActivity::new()))
                    .await
            }
            KeyEvent::Back => {
                //app.show().await;
            }
        }
    }

    async fn show(&self) {
        let mut display = DISPLAY.lock().await;
        let display = display.as_mut().unwrap();
        let mut outbuffer = [0u8; Version::MAX.buffer_len()];
        let mut tempbuffer = [0u8; Version::MAX.buffer_len()];

        let qr = utils::qrcode::QrCode::encode_text(
            "Hello, world!",
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
        let screen_width = 128;
        let screen_height = 64;
        let qr_size = qr.size() as u32 * scale;
        let offset_x = (screen_width - qr_size as u32) / 2;
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
    }
}
