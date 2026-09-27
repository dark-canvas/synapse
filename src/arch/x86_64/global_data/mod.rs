use crate::arch::x86_64::pager::VirtualAddress;
use crate::pager::Pager;

use crate::sync::cpu_mutex::CpuMutex;
use super::task_map::TaskMap;
use super::X86_PAGER;

const KERNEL_GLOBAL_DATA_BASE: VirtualAddress = VirtualAddress(0xFFFFFF9000000000);

pub struct GlobalData<'a> {
    pub task_map: CpuMutex< TaskMap<'a> >,
}

impl<'a> GlobalData<'a> {
    pub fn init() {
        let num_pages = core::mem::size_of::<GlobalData>() + 4095 / 4096;
        X86_PAGER.get().unwrap().allocate_virtual(num_pages, KERNEL_GLOBAL_DATA_BASE).unwrap();
        unsafe { core::ptr::write_bytes(KERNEL_GLOBAL_DATA_BASE.0 as *mut u8, 0x0, num_pages * 4096); }
    }

    pub fn get() -> &'static mut GlobalData<'a> {
        unsafe { &mut *(KERNEL_GLOBAL_DATA_BASE.0 as *mut GlobalData) }
    }
}