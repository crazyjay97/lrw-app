pub mod qrcode;

/// 这个函数只能被调用一次,多次的结果是一样的!警告!!!
/// 除非把种子放到全局变量里
pub fn rand(dev_eui: &[u8; 8], v: u32) -> u32 {
    let mut seed: u64 = 0;
    for &byte in dev_eui.iter() {
        seed = (seed << 8) | (byte as u64);
    }
    const A: u64 = 1664525;
    const C: u64 = 1013904223;
    const M: u64 = 2_u64.pow(32);
    seed = (A.wrapping_mul(seed).wrapping_add(C)) % M;
    let seed = seed as u32;
    seed % v
}
