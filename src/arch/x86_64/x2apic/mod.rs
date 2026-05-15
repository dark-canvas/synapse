use x2apic::lapic::{LocalApic, LocalApicBuilder, xapic_base};
use super::idt::InterruptIndex;
use super::pager::physical_mirror;

pub fn init() {

    // 1. Determine the base address of the LAPIC
    // This typically requires reading the IA32_APIC_BASE MSR
    let apic_physical_address: u64 = unsafe { xapic_base() };

    // 2. Map the physical address to a virtual address if paging is enabled
    let apic_virtual_address: u64 = physical_mirror(apic_physical_address);

    // 3. Build and initialize the Local APIC
    let mut lapic = LocalApicBuilder::new()
        .timer_vector(InterruptIndex::Timer as usize)    // Set timer vector
        .error_vector(InterruptIndex::Error as usize)    // Set error vector
        .spurious_vector(InterruptIndex::Spurious as usize) // Set spurious interrupt vector
        .set_xapic_base(apic_virtual_address)
        .build()
        .unwrap_or_else(|err| panic!("Failed to build LAPIC: {}", err));

    // 4. Enable the APIC
    unsafe {
        lapic.enable();
    }
}