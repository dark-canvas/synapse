use core::sync::atomic::{AtomicUsize, Ordering};

use x86_64::structures::idt::InterruptStackFrame;
use x2apic::lapic::IpiAllShorthand::AllExcludingSelf;

use crate::arch::x86_64::global_data::GlobalData;
use crate::arch::x86_64::interrupts::{self, InterruptIndex};
use crate::arch::x86_64::smp::CpuState;
use crate::pager::VirtualAddress;

pub fn invalidate_page(addr: VirtualAddress) {
    unsafe {
        core::arch::asm!(
            "invlpg 0({0:r})",
            in(reg) addr.0,
            options(nostack, preserves_flags, att_syntax)
        );
    }
}

pub struct InvalidatePageRequest {
    addr: VirtualAddress,
    waiting_on: AtomicUsize,
}

pub extern "x86-interrupt" fn invalidate_page_handler(_stack_frame: InterruptStackFrame) {
    let global_data = GlobalData::get();
    let invalidate_page_request = &global_data.invalidate_page_request;

    // Safety: The INVALIDATE_PAGE_REQUEST is valid and immutable while waiting_on > 0
    let addr = unsafe {
        core::ptr::read_volatile(&invalidate_page_request.addr)
    };

    invalidate_page(addr);

    invalidate_page_request.waiting_on.fetch_sub(1, Ordering::Release);

    unsafe {
        CpuState::get_local_cpu_state().lapic.end_of_interrupt();
    }
}

pub fn invalidate_page_smp(addr: VirtualAddress) {
    invalidate_page(addr);

    // TODO: thread sync
    let cpu_state = CpuState::get_local_cpu_state();
    let global_data = GlobalData::get();
    let invalidate_page_request = &mut global_data.invalidate_page_request;

    // Acquire lock (disable interrupts on local core first to prevent deadlock!)
    //let _guard = disable_interrupts_and_lock(&SHOOTDOWN_LOCK);
    interrupts::enable();
    unsafe {
        core::ptr::write_volatile(&mut invalidate_page_request.addr.0 as *mut u64, addr.0);
    }

    invalidate_page_request.waiting_on.store(cpu_state.get_num_cpus() - 1, Ordering::Release);

    interrupts::disable();
    unsafe {
        cpu_state.lapic.send_ipi_all(InterruptIndex::InvalidatePage as u8, AllExcludingSelf)
    };

    while invalidate_page_request.waiting_on.load(Ordering::Acquire) != 0 {
        core::hint::spin_loop();
    }
}
