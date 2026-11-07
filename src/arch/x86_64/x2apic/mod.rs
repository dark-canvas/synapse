pub mod timer;

use x2apic::lapic::{LocalApicBuilder, xapic_base};

use super::smp::CpuState;
use super::idt::InterruptIndex;
use super::pager::physical_mirror;

pub fn init() {

    // Get the local apic base address, and convert to a virtual address
    let apic_physical_address: u64 = unsafe { xapic_base() };
    let apic_virtual_address: u64 = physical_mirror(apic_physical_address);

    // build an apic for this CPU
    let lapic = LocalApicBuilder::new()
        .timer_vector(InterruptIndex::Timer as usize)
        .error_vector(InterruptIndex::Error as usize)
        .spurious_vector(InterruptIndex::Spurious as usize)
        .set_xapic_base(apic_virtual_address)
        .build()
        .unwrap_or_else(|err| panic!("Failed to build LAPIC: {}", err));

    // save off the apic in per-cpu state so we can use it later
    let cpu_state = CpuState::get_local_cpu_state();
    unsafe {
        let lapic_ptr = &raw mut cpu_state.lapic;
        lapic_ptr.write(lapic);
    }

    // enable the apic via the per-cpu config
    unsafe {
        cpu_state.lapic.enable();
        cpu_state.lapic.disable_timer();
    }

    // Note that the x2apic crate has functionality for starting up the APs as well.
    // I currently do it myself in the smp module.
    // https://docs.rs/x2apic/latest/x2apic/lapic/struct.LocalApic.html#method.send_ipi
    // https://docs.rs/x2apic/latest/x2apic/lapic/struct.LocalApic.html#method.send_sipi
}