#![no_std]
#![no_main]

mod fmt;

use core::cell::RefCell;
use cortex_m_rt::{entry, exception};
use embassy_boot_stm32::*;
use embassy_stm32::flash::{Flash, BANK1_REGION};
use embassy_sync::blocking_mutex::Mutex;

#[cfg(feature = "defmt")]
use defmt_rtt as _;

#[entry]
fn main() -> ! {
    let p = embassy_stm32::init(Default::default());

    let layout = Flash::new_blocking(p.FLASH);
    let flash = Mutex::new(RefCell::new(layout));
    let config = BootLoaderConfig::from_linkerfile_blocking(&flash, &flash, &flash);
    let active_offset = config.active.offset();
    let bl = BootLoader::prepare::<_, _, _, 2048>(config);
    unsafe { bl.load(BANK1_REGION.base + active_offset) }
}

#[no_mangle]
#[cfg_attr(target_os = "none", link_section = ".HardFault.user")]
unsafe extern "C" fn HardFault() {
    cortex_m::peripheral::SCB::sys_reset();
}

#[exception]
unsafe fn DefaultHandler(_: i16) -> ! {
    const SCB_ICSR: *const u32 = 0xE000_ED04 as *const u32;
    let irqn = core::ptr::read_volatile(SCB_ICSR) as u8 as i16 - 16;

    panic!("DefaultHandler #{:?}", irqn);
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    cortex_m::asm::udf();
}
