use crate::arch::x86_64::X86_PAGER;

// TODO: implemenent and use something like ConfigPage in satus

pub struct CpuState {
    pub apic_id: u8,
}

impl CpuState {
    pub fn new() -> &'static CpuState {
        let per_cpu_address = X86_PAGER.get().unwrap().allocate_4kb_page().unwrap();

        let cpu_state: &'static CpuState = unsafe { &*(per_cpu_address as *const CpuState) };

        cpu_state
    }
}
