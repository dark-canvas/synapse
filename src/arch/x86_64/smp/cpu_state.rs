
use num_traits::AsPrimitive;
use satus_struct::config::Config;
use core::default::Default;
use core::arch::asm;
use core::sync::atomic::Ordering;
use num_traits::PrimInt;
use x2apic::lapic::LocalApic;
use crate::Address;
use crate::arch::x86_64::scheduler::Scheduler;
use crate::arch::x86_64::scheduler::TaskHandle;
use crate::smp::SmpError;
use crate::types::CpuId;
use crate::errors::ErrCode;
use super::per_cpu_data;

#[allow(dead_code)]
#[derive(Default, PartialEq, Clone, Copy)]
pub enum State {
    #[default] Off,
    Initializing,
    Initialized,
}

#[repr(C)]
pub struct CpuState {
    pub apic_id: u8, // TODO: CpuId (but stored as a u8)
    pub state: State,
    // u8 here
    // u8 here
    // u32 here
    pub current_tid: TaskHandle, // u64
    pub config: Address, // Address of config from bootloader (struct needs to be default initializeable)
    pub scheduler: Scheduler,
    pub lapic: LocalApic,
    pub lapic_timer_ticks_per_ms: u32,
}

impl CpuState {
    pub fn get(id: CpuId) -> &'static mut CpuState {
        let base_address = per_cpu_data::get_cpu_state_base(id);
        unsafe { &mut *(base_address.0 as *mut CpuState) }
    }

    pub fn get_local_cpu_state() -> &'static mut CpuState {
        let base_address: usize;
        unsafe {
            asm!(
                "rdgsbase {}",
                out(reg) base_address,
                options(nostack, att_syntax)
            );
        }
        unsafe { &mut *(base_address as *mut CpuState) }
    }

    pub fn wait_for_others(&self) {
        let num_cpus = self.get_num_cpus();
        let mut matched = 0;
        while matched != num_cpus {
            matched = 0;
            for cpu in 0..num_cpus {
                let cpu_state = CpuState::get(cpu.try_into().unwrap());

                if cpu_state.state == self.state {
                    matched += 1;
                }
            }
            core::hint::spin_loop();
        }
    }

    pub fn get_cpu_id(&self) -> CpuId {
        self.apic_id as CpuId
    }

    pub fn get_bootloader_config(&self) -> Config {
        Config::from_page(self.config)
    }

    pub fn get_num_cpus(&self) -> usize {
        self.get_bootloader_config().get_cpu_config().get_num_cpus() as usize
    }
}

// TODO: modify read (movq) based on size of T?
#[allow(dead_code)]
unsafe fn get_cpu_state_at_offset<T: PrimInt + 'static>(offset: usize) -> T
where
    u64: AsPrimitive<T>,
{
    let mut val: u64;
    unsafe {
        core::arch::asm!(
            "movq %gs:(,{offset}), {val}",
            offset = in(reg) offset,
            val = out(reg) val,
            options(att_syntax),
        );
    }

    val.as_()
}
    
// TODO: proper error codes for this... (return err if SMP not initialized?)
// Or just assert, since it just shouldn't be called in that case, and can be avoided
#[allow(dead_code)]
pub fn get_cpu_id() -> Result<CpuId, ErrCode> {
    if !super::SMP_INITIALIZED.load(Ordering::Relaxed) {
        return Err(ErrCode::Smp(SmpError::NotInitialized));
    }
    Ok( unsafe { get_cpu_state_at_offset::<u8>( core::mem::offset_of!(CpuState, apic_id) ) } as CpuId )
}

/* 
pub fn get_current_task_id() -> TaskId {
    ( unsafe { get_cpu_state_at_offset::<u64>( core::mem::offset_of!(CpuState, current_tid) ) }) as TaskId
}
*/
