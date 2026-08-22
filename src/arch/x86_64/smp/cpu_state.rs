use crate::arch::x86_64::X86_PAGER;
use crate::page_based;
use crate::types::CpuId;
use crate::errors::ErrCode;
use core::default::Default;
use core::arch::asm;
use num_traits::PrimInt;

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

    // TODO: pick a standard form for inline assembly (AT&T... look into the nostack, pure, readonly variables as well)
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

// TODO: modify read (movq) based on size of T?
unsafe fn get_cpu_state_at_offset<T: PrimInt>(offset: usize) -> T {
    let mut val: u64;
    core::arch::asm!(
        "movq %gs:(,{offset}), {val}",
        offset = in(reg) offset,
        val = out(reg) val,
        options(att_syntax),
    );
    num_traits::cast(val).unwrap()
}
    
// TODO: proper error codes for this... (return err if SMP not initialized?)
pub fn get_cpu_id() -> Result<CpuId, ErrCode> {
    Ok( unsafe { get_cpu_state_at_offset::<u8>(0) } as CpuId )
}
