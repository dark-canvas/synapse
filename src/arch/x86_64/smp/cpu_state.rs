use crate::page_based;
use crate::types::CpuId;
use crate::errors::ErrCode;
use super::per_cpu_data;
use core::default::Default;
use core::arch::asm;
use num_traits::PrimInt;

// TODO: implemenent and use something like ConfigPage in satus?

#[allow(dead_code)]
#[derive(Default, PartialEq, Clone, Copy)]
pub enum State {
    #[default] Off,
    Initializing,
    Initialized,
}

#[repr(C)]
#[derive(Default)]
pub struct CpuState {
    pub apic_id: u8, // TODO: CpuId (but stored as a u8)
    pub state: State,
}

impl CpuState {
    pub fn new(id: CpuId) -> &'static mut CpuState {
        page_based::allocator::new_at::<CpuState>( per_cpu_data::get_cpu_state_base(id) )
    }

    pub fn get(id: CpuId) -> &'static mut CpuState {
        let base_address = per_cpu_data::get_cpu_state_base(id);
        unsafe { &mut *(base_address.0 as *mut CpuState) }
    }

    // TODO: pick a standard form for inline assembly (AT&T... look into the nostack, pure, readonly variables as well)
    pub unsafe fn get_local_cpu_state() -> &'static mut CpuState {
        let base_address: usize;
        unsafe {
            asm!(
                "rdgsbase {}",
                out(reg) base_address,
                options(nostack/*, pure, readonly*/)
            );
        }
        unsafe { &mut *(base_address as *mut CpuState) }
    }
}

// TODO: modify read (movq) based on size of T?
#[allow(dead_code)]
unsafe fn get_cpu_state_at_offset<T: PrimInt>(offset: usize) -> T {
    let mut val: u64;
    unsafe {
        core::arch::asm!(
            "movq %gs:(,{offset}), {val}",
            offset = in(reg) offset,
            val = out(reg) val,
            options(att_syntax),
        );
    }
    num_traits::cast(val).unwrap()
}
    
// TODO: proper error codes for this... (return err if SMP not initialized?)
#[allow(dead_code)]
pub fn get_cpu_id() -> Result<CpuId, ErrCode> {
    Ok( unsafe { get_cpu_state_at_offset::<u8>(0) } as CpuId )
}
