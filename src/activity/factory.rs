use core::{borrow::Borrow, str::FromStr};

use crate::{
    info,
    lorawan::factory,
    serial::{send_command, Command, GetDevEuiResult},
};

use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    mono_font::{ascii::FONT_7X13, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use heapless::String;

use super::{Activity, DISPLAY};

pub struct FactoryActivity {
    state: Mutex<ThreadModeRawMutex, FactoryState>,
}

enum FactoryState {
    Ready,
    Factorying,
    Succeed,
}

impl FactoryActivity {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(FactoryState::Ready),
        }
    }
}

impl Activity for FactoryActivity {
    async fn key_handle(&self, e: crate::AppEvent, app: &super::App) {
        match e {
            crate::AppEvent::Prev => {
                let eui: Result<GetDevEuiResult, ()> =
                    send_command(Command::GetDevEui, Duration::from_millis(300)).await;
                if let Ok(eui) = eui {
                    info!("eui: {:?}", eui.0);
                } else {
                    info!("eui: failed");
                }
            }
            crate::AppEvent::Next => {}
            crate::AppEvent::Confirm => {
                {
                    *self.state.lock().await = FactoryState::Factorying;
                }
                self.show().await;
                factory().await;
                Timer::after(Duration::from_millis(500)).await;
                {
                    *self.state.lock().await = FactoryState::Succeed;
                }
                self.show().await;
            }
            crate::AppEvent::Back => {}
            crate::AppEvent::Message(_, _) => {}
        }
    }

    async fn show(&self) {
        let mut display = DISPLAY.lock().await;
        let display = display.as_mut().unwrap();
        let _ = display.clear(BinaryColor::Off);
        let character_style = MonoTextStyleBuilder::new()
            .font(&FONT_7X13)
            .text_color(BinaryColor::On)
            .build();
        let left_aligned = TextStyleBuilder::new()
            .alignment(Alignment::Center)
            .baseline(Baseline::Middle)
            .build();
        let state = {
            let state = self.state.lock().await;
            match &*state {
                FactoryState::Ready => FactoryState::Ready,
                FactoryState::Factorying => FactoryState::Factorying,
                FactoryState::Succeed => FactoryState::Succeed,
            }
        };
        match state {
            FactoryState::Ready => {
                let _ = Text::with_text_style(
                    "Confirm to factory",
                    display.bounding_box().center(),
                    character_style,
                    left_aligned,
                )
                .draw(display);
            }
            FactoryState::Factorying => {
                let title = String::<10>::from_str("Factorying").unwrap();
                let _ = Text::with_text_style(
                    &title.as_str(),
                    display.bounding_box().center(),
                    character_style,
                    left_aligned,
                )
                .draw(display);
            }
            FactoryState::Succeed => {
                let _ = Text::with_text_style(
                    "Factory Succeed",
                    display.bounding_box().center(),
                    character_style,
                    left_aligned,
                )
                .draw(display);
            }
        }
        let _ = display.flush().await;
    }
}
