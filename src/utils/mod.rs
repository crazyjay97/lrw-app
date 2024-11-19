use embassy_sync::{blocking_mutex::raw::ThreadModeRawMutex, mutex::Mutex};

pub mod qrcode;

static SEED: Mutex<ThreadModeRawMutex, u64> = Mutex::new(0);

pub async fn init_seed(dev_eui: &[u8; 8], seed_v: u64) {
    let mut seed_eui: u64 = 0;
    for &byte in dev_eui.iter() {
        seed_eui = (seed_eui << 8) | (byte as u64);
    }
    let seed_eui = calc_rand(seed_eui);
    let seed_v = calc_rand(seed_v);
    {
        *SEED.lock().await = seed_eui + seed_v;
    }
    {
        *SEED.lock().await = rand(u64::MAX).await as u64;
    }
}

fn calc_rand(seed: u64) -> u64 {
    const A: u64 = 1664525;
    const C: u64 = 1013904223;
    const M: u64 = 2_u64.pow(32);
    let seed = (A.wrapping_mul(seed).wrapping_add(C)) % M;
    seed
}

pub async fn rand(v: u64) -> u64 {
    let seed = { *SEED.lock().await };
    let seed = calc_rand(seed);
    let num = seed % v;
    {
        *SEED.lock().await = num as u64
    }
    num
}
