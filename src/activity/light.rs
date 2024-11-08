use crate::{
    info,
    lorawan::{into_lorawan_mode, LoRaWANPackage, LoRaWANState},
    AppEvent,
};
use core::cell::RefCell;
use core::fmt::Write;
use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, channel::Channel};
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::ascii::{FONT_4X6, FONT_7X13},
    mono_font::MonoTextStyleBuilder,
    pixelcolor::BinaryColor,
    prelude::Dimensions,
    prelude::{Point, Primitive, Size},
    primitives::{PrimitiveStyleBuilder, Rectangle},
    text::TextStyleBuilder,
    text::{Alignment, Baseline, Text},
    Drawable,
};
use heapless::String;

use super::{Activity, DISPLAY};

pub struct LightActivity {
    rssi: RefCell<Option<i8>>,
    snr: RefCell<Option<i8>>,
    state: RefCell<LoRaWANState>,
    chan: Channel<ThreadModeRawMutex, u8, 1>,
    light: RefCell<bool>,
}

impl LightActivity {
    pub fn new() -> Self {
        Self {
            rssi: RefCell::new(None),
            snr: RefCell::new(None),
            state: RefCell::new(LoRaWANState::Ready),
            chan: Channel::new(),
            light: RefCell::new(false),
        }
    }
}

impl Activity for LightActivity {
    async fn key_handle(&self, e: AppEvent, _: &super::App) {
        match e {
            AppEvent::Prev => todo!(),
            AppEvent::Next => todo!(),
            AppEvent::Confirm => {
                self.state.replace(LoRaWANState::Joining);
                self.refresh().await;
                if into_lorawan_mode().await {
                    info!("join success");
                } else {
                    info!("join failed");
                }
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
                    if data.data[0] == 0xFF {
                        self.light.replace(true);
                    } else {
                        self.light.replace(false);
                    }
                }
                self.refresh().await;
            }
        }
    }

    async fn show(&self) {
        loop {
            let mut display = DISPLAY.lock().await;
            let display = display.as_mut().unwrap();
            let _ = display.clear(BinaryColor::Off);

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
                let _ = Text::with_text_style(s.as_str(), Point::new(128, 0), style, aligned)
                    .draw(display);
            }
            let character_style = MonoTextStyleBuilder::new()
                .font(&FONT_7X13)
                .text_color(BinaryColor::On)
                .build();
            let left_aligned = TextStyleBuilder::new()
                .alignment(Alignment::Center)
                .baseline(Baseline::Bottom)
                .build();
            let _ = Text::with_text_style(
                "LoRaWAN",
                display.bounding_box().center(),
                character_style,
                left_aligned,
            )
            .draw(display);
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
                let _ = Rectangle::new(Point::new(0, 40), Size::new(128, 24))
                    .into_styled(style)
                    .draw(display);
            }
            let _ = display.flush().await;
            let _ = self.chan.receive().await;
        }
    }
}

impl LightActivity {
    pub async fn refresh(&self) {
        self.chan.send(0).await;
    }
}
