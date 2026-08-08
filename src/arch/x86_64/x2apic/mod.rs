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

    // start-up all CPUs:
    /*
    // Configure the ICR for an INIT IPI targeting a specific x2APIC ID
    unsafe {
        lapic.send_ipi(
            target_lapic_id,
            x2apic::lapic::IpiDeliveryMode::Init,
            x2apic::lapic::IpiDestinationShorthand::NoShorthand,
            0, // Vector is 0 for INIT
        );
    }
    */

    // Wait approximately 10 milliseconds for the core to process the reset.

    /*
    // The vector determines the entry address: Address = Vector * 0x1000
    // For example, Vector 0x08 tells the AP to boot at address 0x8000
    let boot_vector = 0x08; 

    unsafe {
        lapic.send_ipi(
            target_lapic_id,
            x2apic::lapic::IpiDeliveryMode::StartUp,
            x2apic::lapic::IpiDestinationShorthand::NoShorthand,
            boot_vector,
        );
    }
    */

    // Wait 200 microseconds. If the core has not updated a pre-defined shared 
    // memory flag to signify it is awake, send a second identical SIPI.
}