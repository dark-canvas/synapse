use crate::Address;
use crate::page_based;
use crate::types::CpuId;
use crate::errors::ErrCode;
use super::per_cpu_data;
use satus_struct::config::Config;
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
    pub config: Address, // Address of config from bootloader (struct needs to be default initializeable)
}

impl CpuState {
    // or set config after?
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

    pub fn get_cpu_id(&self) -> CpuId {
        self.apic_id as CpuId
    }

    pub fn get_bootloader_config(&self) -> Config {
        Config::from_page(self.config)
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
