use crate::types::CpuId;
use super::per_cpu_data;
use crate::pager::PAGER;
use crate::arch::x86_64::smp::VirtualAddress;


pub struct CpuStack {
    pub base: VirtualAddress,   
}

impl CpuStack {
    pub fn new(id: CpuId) -> CpuStack {
        let pager = PAGER.borrow();

        let stack_top = per_cpu_data::get_stack_top(id);
        let num_pages = per_cpu_data::STACK_SIZE / pager.get_page_size();
        pager.allocate_virtual(num_pages, stack_top).unwrap();

        let stack_bytes = stack_top.0 as *mut u8;
        unsafe {
            stack_bytes.write_bytes(0x55, per_cpu_data::STACK_SIZE);
        }

        Self::get(id)
    }

    pub fn get(id: CpuId) -> CpuStack {
        CpuStack {
            base:  per_cpu_data::get_stack_base(id)
        }
    }

    pub fn get_top(&self) -> VirtualAddress {
        self.base + per_cpu_data::STACK_SIZE
    }
}