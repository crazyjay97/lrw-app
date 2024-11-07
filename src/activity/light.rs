use crate::{info, lorawan::into_lorawan_mode, serial::rx_listen};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    draw_target::DrawTarget,
    text::{Alignment, Baseline, Text},
    Drawable,
};
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::Dimensions,
    text::TextStyleBuilder,
};

use super::{Activity, DISPLAY};

pub struct LightActivity {}

impl LightActivity {
    pub fn new() -> Self {
        Self {}
    }
}

impl Activity for LightActivity {
    async fn key_handle(&self, e: crate::KeyEvent, app: &super::App) {
        match e {
            crate::KeyEvent::Prev => todo!(),
            crate::KeyEvent::Next => todo!(),
            crate::KeyEvent::Confirm => {
                into_lorawan_mode().await;
                loop {
                    let (rx, len) = rx_listen().await;
                    info!("rx: {:?}", core::str::from_utf8(&rx[0..len]).unwrap());
                    Timer::after(Duration::from_millis(100)).await;
                }
            }
            crate::KeyEvent::Back => todo!(),
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
        let _ = Text::with_text_style(
            "LoRaWAN",
            display.bounding_box().center(),
            character_style,
            left_aligned,
        )
        .draw(display);
        let _ = display.flush().await;
    }
}



/// LoRaWAN