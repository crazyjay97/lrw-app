use crate::serial::uart1_write;

use embedded_graphics::{
    mono_font::{iso_8859_1::FONT_10X20, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};

use super::{Activity, DISPLAY};

pub struct FactoryActivity {}

impl FactoryActivity {
    pub fn new() -> Self {
        Self {}
    }
}

impl Activity for FactoryActivity {
    async fn key_handle(&self, e: crate::KeyEvent, app: &super::App) {
        match e {
            crate::KeyEvent::Prev => {
                let _ = uart1_write(b"at+deveui?\r\n").await;
            }
            crate::KeyEvent::Next => {}
            crate::KeyEvent::Confirm => {}
            crate::KeyEvent::Back => {}
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
            "Factory.....",
            display.bounding_box().center(),
            character_style,
            left_aligned,
        )
        .draw(display);
        let _ = display.flush().await;
    }
}
