use crate::arch::x86_64::X86_PAGER;
use crate::page_based;
use core::default::Default;

// TODO: implemenent and use something like ConfigPage in satus

#[derive(Default)]
pub struct CpuState {
    pub apic_id: u8,
}

impl CpuState {
    pub fn new() -> &'static CpuState {
        page_based::allocator::new::<CpuState>()
    }
}
