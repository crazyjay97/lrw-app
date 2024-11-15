use heapless::Vec;

use crate::info;

pub struct Stack<const D: usize, T: StackData<D>> {
    head: u8,
    cmd: u8,
    addr: Addr,
    data: T,
    end: u8,
}

impl<const D: usize, T: StackData<D>> Stack<D, T> {
    const HEAD: u8 = 0x68;
    const END: u8 = 0x16;

    pub fn new(cmd: Cmd, data: T, addr: Addr) -> Self {
        Self {
            head: Self::HEAD,
            cmd: cmd.as_u8(),
            data,
            addr,
            end: Self::END,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8, 64> {
        let mut buf = Vec::<u8, 64>::new();
        let _ = buf.push(self.head);
        let _ = buf.push(self.cmd);
        let addr = &self.addr.0;
        info!("addr: {:02X}", addr);
        let _ = buf.extend_from_slice(addr);
        if D > 0 {
            let _ = buf.extend_from_slice(&self.data.to_bytes());
        }
        let _ = buf.push(self.end);
        buf
    }
}

pub trait StackData<const S: usize> {
    fn to_bytes(&self) -> [u8; S];
}

impl StackData<0> for () {
    fn to_bytes(&self) -> [u8; 0] {
        []
    }
}
pub struct Addr([u8; 16]);
impl Addr {
    pub fn new(addr: [u8; 16]) -> Self {
        Self(addr)
    }
}

pub enum Cmd {
    ApplyCode,
    ApplyCodeResp,
    Control,
    Event,
    Heartbeat,
}

enum State {
    Close,
    Open,
}

impl Cmd {
    fn as_u8(&self) -> u8 {
        match self {
            Cmd::ApplyCode => 0x01,
            Cmd::ApplyCodeResp => 0x81,
            Cmd::Event => 0x02,
            Cmd::Control => 0x82,
            Cmd::Heartbeat => 0x06,
        }
    }
}

pub fn get_apply_code_cmd() -> Stack<0, ()> {
    Stack::new(Cmd::ApplyCode, (), Addr([0; 16]))
}

pub struct Heartbeat {
    pub light: u8,
    pub brightness: u8,
}

impl StackData<2> for Heartbeat {
    fn to_bytes(&self) -> [u8; 2] {
        [self.light, self.brightness]
    }
}

pub fn pack_heartbeat(heartbeat: Heartbeat) -> Stack<2, Heartbeat> {
    Stack::new(Cmd::Heartbeat, heartbeat, Addr([0; 16]))
}
