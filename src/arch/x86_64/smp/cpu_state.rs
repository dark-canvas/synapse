use crate::arch::x86_64::X86_PAGER;
use crate::page_based;
use core::default::Default;
use core::arch::asm;

// TODO: implemenent and use something like ConfigPage in satus?

#[derive(Default)]
pub enum State {
    #[default] Off,
    Initializing,
    Initialized,
}

#[repr(C)]
#[derive(Default)]
pub struct CpuState {
    pub apic_id: u8,
    pub state: State,
}

impl CpuState {
    pub fn new() -> &'static mut CpuState {
        page_based::allocator::new::<CpuState>()
    }

    pub unsafe fn get_local_cpu_state() -> &'static mut CpuState {
        let base_address: usize;
        asm!(
            "rdgsbase {}",
            out(reg) base_address,
            options(nostack, pure, readonly)
        );
        &mut *(base_address as *mut CpuState)
    }
}
