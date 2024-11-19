use crate::{
    activity::EuiQrCodeActivity,
    config, info,
    lorawan::{LoRaWANPackage, LoRaWANState},
    proto::{pack_heartbeat, Heartbeat},
    serial::uart1_write,
    AppEvent, RE_JOIN_CHANNEL,
};
use core::cell::RefCell;
use core::fmt::Write;
use embassy_futures::select;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{
        ascii::{FONT_4X6, FONT_5X7},
        MonoTextStyleBuilder,
    },
    pixelcolor::BinaryColor,
    prelude::{Dimensions, Point, Primitive, Size},
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::{Alignment, Baseline, Text, TextStyleBuilder},
    Drawable,
};
use heapless::{String, Vec};
use ssd1306::prelude::Brightness;

use super::{Activity, DISPLAY};

pub struct LightActivity {
    rssi: RefCell<Option<i8>>,
    snr: RefCell<Option<i8>>,
    state: RefCell<LoRaWANState>,
    chan: Channel<ThreadModeRawMutex, u8, 1>,
    light: RefCell<bool>,
    load: RefCell<bool>,
    brightness: RefCell<u8>,
    send_heartbeat: RefCell<bool>,
}

impl LightActivity {
    pub fn new() -> Self {
        Self {
            rssi: RefCell::new(None),
            snr: RefCell::new(None),
            state: RefCell::new(LoRaWANState::Online),
            chan: Channel::new(),
            light: RefCell::new(false),
            load: RefCell::new(false),
            brightness: RefCell::new(0x5F),
            send_heartbeat: RefCell::new(false),
        }
    }
}

impl Activity for LightActivity {
    async fn key_handle(&self, e: AppEvent, app: &super::App) {
        match e {
            AppEvent::Prev => todo!(),
            AppEvent::Next => todo!(),
            AppEvent::Confirm => {
                {
                    self.state.replace(LoRaWANState::Online);
                }
                self.refresh().await;
            }
            AppEvent::Back => todo!(),
            AppEvent::Message(buf, len) => {
                let data = LoRaWANPackage::decode(&buf[0..len]);
                info!(
                    "downlink: {:02X} rssi: {} snr: {}",
                    data.data, data.rssi, data.snr
                );
                {
                    self.rssi.replace(Some(data.rssi));
                    self.snr.replace(Some(data.snr));
                    let l = data.data.len();
                    if data.data[0] == 0x68 && data.data[l - 1] == 0x16 && l >= 19 {
                        let cmd = data.data[1];
                        let addr: &[u8] = &data.data[2..18];
                        if cmd == 0x81 {
                            let rs = config::write_config(config::Config {
                                code: Vec::<u8, 16>::from_slice(&addr).unwrap(),
                            })
                            .await;
                            info!("set code: {:?}", rs.is_ok());
                        } else {
                            let next = {
                                let mut next = false;
                                let config = config::CONFIG.lock().await;
                                let code = config.as_ref().unwrap().code.as_slice();
                                for i in 0..code.len() {
                                    if code[i] & addr[i] > 0 {
                                        next = true;
                                        break;
                                    }
                                }
                                next
                            };
                            if next {
                                if cmd == 0x82 {
                                    let control = data.data[18];
                                    if control == 0x01 {
                                        self.light.replace(true);
                                    }
                                    if control == 0x02 {
                                        self.light.replace(false);
                                    }
                                } else if cmd == 0x83 {
                                    if !*self.load.borrow_mut() {
                                        self.load.replace(true);
                                    } else {
                                        return;
                                    }
                                } else if cmd == 0x84 {
                                    let brightness = data.data[18];
                                    if next {
                                        self.brightness.replace(brightness);
                                    }
                                } else if cmd == 0x85 {
                                    self.done().await;
                                    let _ = RE_JOIN_CHANNEL.send(1).await;
                                    app.navigate_to(crate::activity::AppActivity::EuiQrCode(
                                        EuiQrCodeActivity::new(),
                                    ))
                                    .await;
                                    return;
                                } else if cmd == 0x86 {
                                    let control = data.data[18];
                                    if control == 0x01 {
                                        self.send_heartbeat.replace(true);
                                    }
                                    if control == 0x02 {
                                        self.send_heartbeat.replace(false);
                                    }
                                    return;
                                }
                            }
                        }
                    }
                }
                self.refresh().await;
            }
            _ => {}
        }
    }

    async fn show(&self) {
        select::select(
            async {
                loop {
                    let send_heartbeat = { *self.send_heartbeat.borrow_mut() };
                    if send_heartbeat {
                        let brightness = { *self.brightness.borrow() };
                        let light = { *self.light.borrow() };
                        let pack = pack_heartbeat(Heartbeat {
                            light: if light { 0x01 } else { 0x02 },
                            brightness,
                        });
                        let _ = uart1_write(&pack.to_bytes()).await;
                    }
                    let _ = Timer::after(Duration::from_millis(30000)).await;
                }
            },
            async {
                loop {
                    let mut display = DISPLAY.lock().await;
                    let display = display.as_mut().unwrap();
                    let _ = display.clear(BinaryColor::Off);
                    let brightness = {
                        let brightness = self.brightness.borrow();
                        *brightness
                    };
                    let _ = display
                        .set_brightness(Brightness::custom(2, brightness))
                        .await;
                    if let Some(rssi) = self.rssi.borrow().as_ref() {
                        let snr = self.snr.borrow().unwrap();
                        let style = MonoTextStyleBuilder::new()
                            .font(&FONT_4X6)
                            .text_color(BinaryColor::On)
                            .build();
                        let aligned = TextStyleBuilder::new()
                            .alignment(Alignment::Left)
                            .baseline(Baseline::Top)
                            .build();
                        let mut s = String::<16>::new();
                        let _ = write!(s, "r: {} s: {}", rssi, snr);
                        let _ = Text::with_text_style(
                            s.as_str(),
                            display.bounding_box().top_left,
                            style,
                            aligned,
                        )
                        .draw(display);
                    }

                    {
                        let state = self.state.borrow();
                        let style = MonoTextStyleBuilder::new()
                            .font(&FONT_4X6)
                            .text_color(BinaryColor::On)
                            .build();
                        let aligned = TextStyleBuilder::new()
                            .alignment(Alignment::Right)
                            .baseline(Baseline::Top)
                            .build();
                        let mut s = String::<16>::new();
                        let _ = write!(s, "{}", state.to_str());
                        let _ =
                            Text::with_text_style(s.as_str(), Point::new(128, 0), style, aligned)
                                .draw(display);
                    }
                    let character_style = MonoTextStyleBuilder::new()
                        .font(&FONT_5X7)
                        .text_color(BinaryColor::On)
                        .build();
                    let left_aligned = TextStyleBuilder::new()
                        .alignment(Alignment::Center)
                        .baseline(Baseline::Bottom)
                        .build();
                    let mut center = display.bounding_box().center();
                    center.y = 15;
                    let _ = Text::with_text_style("LoRaWAN", center, character_style, left_aligned)
                        .draw(display);
                    {
                        let load = { *self.load.borrow_mut() };
                        if !load {
                            {
                                let light = self.light.borrow();
                                let style = PrimitiveStyleBuilder::new()
                                    .fill_color(if *light {
                                        BinaryColor::On
                                    } else {
                                        BinaryColor::Off
                                    })
                                    .stroke_width(1)
                                    .stroke_color(BinaryColor::On)
                                    .build();
                                let _ = Rectangle::new(Point::new(0, 16), Size::new(128, 48))
                                    .into_styled(style)
                                    .draw(display);
                            }
                        } else {
                            {
                                let mut w = 1;
                                loop {
                                    if w >= 128 {
                                        break;
                                    }
                                    let style = PrimitiveStyleBuilder::new()
                                        .fill_color(BinaryColor::Off)
                                        .stroke_width(1)
                                        .stroke_color(BinaryColor::On)
                                        .build();
                                    let _ = Rectangle::new(Point::new(0, 16), Size::new(128, 48))
                                        .into_styled(style)
                                        .draw(display);
                                    let fill_style = PrimitiveStyleBuilder::new()
                                        .fill_color(BinaryColor::On)
                                        .stroke_width(1)
                                        .stroke_color(BinaryColor::On)
                                        .build();
                                    let _ = Rectangle::new(Point::new(0, 16), Size::new(w, 48))
                                        .into_styled(fill_style)
                                        .draw(display);
                                    let _ = display.flush().await;
                                    Timer::after(Duration::from_millis(80)).await;
                                    info!("w: {}", w);
                                    w += 1;
                                }
                            }
                            {
                                *self.load.borrow_mut() = false
                            };
                        }
                    }
                    let _ = display.flush().await;
                    let c = self.chan.receive().await;
                    if c == 0xFF {
                        return;
                    }
                }
            },
        )
        .await;
    }
}

impl LightActivity {
    pub async fn refresh(&self) {
        self.chan.send(0).await;
    }

    pub async fn done(&self) {
        self.chan.send(0xFF).await;
        Timer::after(Duration::from_millis(50)).await;
    }
}
