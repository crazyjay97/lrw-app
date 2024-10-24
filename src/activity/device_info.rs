use core::{cell::RefCell, cmp::min};

use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyleBuilder},
    pixelcolor::BinaryColor,
    prelude::*,
    text::{Alignment, Baseline, Text, TextStyleBuilder},
};
use heapless::String;

use crate::{lorawan::LORAWAN, Ssd1306DisplayType};

use super::{info, Activity, DISPLAY};

pub struct DeviceInfoActivity {
    label_list: &'static [&'static str],
    current_index: RefCell<usize>,
    start: RefCell<usize>,
    end: RefCell<usize>,
}

impl Activity for DeviceInfoActivity {
    async fn key_handle(&self, e: crate::KeyEvent, app: &super::App) {
        match e {
            crate::KeyEvent::Prev => {
                self.draw_list(1).await;
            }
            crate::KeyEvent::Next => {
                self.draw_list(-1).await;
            }
            crate::KeyEvent::Confirm => {}
            crate::KeyEvent::Back => {}
        }
    }

    async fn show(&self) {
        self.draw_list(0).await;
    }
}

impl DeviceInfoActivity {
    const WIDTH: i32 = 128;
    const HEIGHT: i32 = 64;
    const LABEL_WIDTH: i32 = 60;
    const LABEL_PADDING: i32 = 2;
    const FONT_WIDTH: i32 = 6;
    pub fn new() -> Self {
        Self {
            label_list: &[
                "Type", "State", "DevEui", "Appeui", "DevAddr", "Version", "Class", "Appskey",
                "Newkskey",
            ],
            current_index: RefCell::new(0),
            start: RefCell::new(0),
            end: RefCell::new(1),
        }
    }
    async fn draw_list(&self, dire: i8) {
        let mut display = DISPLAY.lock().await;
        let mut display = display.as_mut().unwrap();
        let mut selected = self.current_index.borrow_mut();
        let mut start = self.start.borrow_mut();
        let mut end = self.end.borrow_mut();
        if (dire < 0 && *selected == 0)
            || (dire > 0 && *selected == (self.label_list.len() - 1) as usize)
        {
            return;
        }
        let _ = display.clear(BinaryColor::Off);
        if *start == *selected && dire < 0 {
            info!("to left <<<<<<<<<<<<<<<<<<<<< ");
            *selected -= 1;
            *start -= 1;
        } else if (*end - 1) == *selected && dire > 0 {
            info!("to right >>>>>>>>>>>>>>>>>>>>>>>>.");
            *selected += 1;
            *start += 1;
        } else {
            info!("move cursor");
            if dire > 0 {
                *selected += 1;
            } else if dire < 0 {
                *selected -= 1;
            }
        }
        // 计算移动以后有几个label可以显示在屏幕上
        let mut item_max = 0;
        let mut total_width = 0;
        for i in *start..self.label_list.len() {
            if (total_width + Self::FONT_WIDTH * self.label_list[i].len() as i32)
                + ((item_max - 1) * Self::LABEL_PADDING)
                > Self::WIDTH
            {
                break;
            }
            total_width += Self::FONT_WIDTH * self.label_list[i].len() as i32;
            item_max += 1;
        }
        *end = min(*start + item_max as usize, self.label_list.len());
        info!(
            "selected: {} start: {} end: {} item_max: {}",
            *selected, *start, *end, item_max
        );
        let show_next = Self::WIDTH - (total_width + ((item_max - 1) * Self::LABEL_PADDING))
            > Self::LABEL_PADDING;
        let final_end = if show_next {
            min(*end + 1, self.label_list.len() as usize)
        } else {
            *end
        };
        self.draw_and_move(&mut display, *start, final_end, *selected)
            .await;
        let label = self.label_list[*selected];
        let character_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(BinaryColor::On)
            .build();
        let left_aligned = TextStyleBuilder::new()
            .alignment(Alignment::Left)
            .baseline(Baseline::Middle)
            .build();
        let lrw = LORAWAN.lock().await;
        let lrw = lrw.as_ref().unwrap();
        let value = match label {
            "Type" => lrw.join_type.as_str(),
            "State" => lrw.state.as_str(),
            "DevEui" => lrw.deveui.as_ref().unwrap().as_str(),
            "Appeui" => {
                if lrw.appeui.is_some() {
                    lrw.appeui.as_ref().unwrap().as_str()
                } else {
                    ""
                }
            }
            "Version" => {
                if lrw.version.is_some() {
                    lrw.version.as_ref().unwrap().as_str()
                } else {
                    ""
                }
            }
            "DevAddr" => {
                if lrw.devaddr.is_some() {
                    lrw.devaddr.as_ref().unwrap().as_str()
                } else {
                    ""
                }
            }
            "Class" => lrw.class.as_str(),
            "Appskey" => {
                if lrw.appskey.is_some() {
                    lrw.appskey.as_ref().unwrap().as_str()
                } else {
                    ""
                }
            }
            "Newkskey" => {
                if lrw.nwkskey.is_some() {
                    lrw.nwkskey.as_ref().unwrap().as_str()
                } else {
                    ""
                }
            }
            _ => lrw.deveui.as_ref().unwrap().as_str(),
        };
        let text = wrap_text_by_char::<256>(&value);
        Text::with_text_style(
            text.as_str(),
            Point { x: 0, y: 32 },
            character_style,
            left_aligned,
        )
        .draw(display)
        .unwrap_or(Point { x: 0, y: 0 });
        let _ = display.flush().await;
    }

    async fn draw_and_move(
        &self,
        display: &mut Ssd1306DisplayType,
        start: usize,
        end: usize,
        selected: usize,
    ) {
        let mut start_x = 0;
        for idx in start..end {
            let point = self
                .draw(display, idx as u8, start_x, idx == selected)
                .await;
            start_x = point.x + Self::LABEL_PADDING;
        }
    }

    async fn draw(
        &self,
        display: &mut Ssd1306DisplayType,
        idx: u8,
        x: i32,
        selected: bool,
    ) -> Point {
        let character_style = MonoTextStyleBuilder::new()
            .font(&FONT_6X10)
            .text_color(if selected {
                BinaryColor::Off
            } else {
                BinaryColor::On
            })
            .background_color({
                if selected {
                    BinaryColor::On
                } else {
                    BinaryColor::Off
                }
            })
            .build();
        let left_aligned = TextStyleBuilder::new()
            .alignment(Alignment::Left)
            .baseline(Baseline::Top)
            .build();
        let label = self.label_list[idx as usize];
        Text::with_text_style(label, Point { x: x, y: 0 }, character_style, left_aligned)
            .draw(display)
            .unwrap_or(Point { x: 0, y: 0 })
    }
}

fn wrap_text_by_char<const N: usize>(text: &str) -> String<N> {
    let max_chars_per_line = (DeviceInfoActivity::WIDTH / DeviceInfoActivity::FONT_WIDTH) as usize;
    let mut lines: String<N> = String::new();
    let mut current_line: String<32> = String::new();

    for (i, c) in text.chars().enumerate() {
        current_line.push(c).unwrap();
        if (i + 1) % max_chars_per_line == 0 {
            lines.push_str(current_line.as_str()).unwrap();
            lines.push('\n').unwrap();
            current_line.clear();
        }
    }
    if !current_line.is_empty() {
        lines.push_str(current_line.as_str()).unwrap();
        lines.push('\n').unwrap();
    }
    lines
}
