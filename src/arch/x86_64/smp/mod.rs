pub mod acpi_handler;

use satus_struct::cpu_config::CpuConfig;
use core::sync::atomic::{AtomicBool, Ordering};
use acpi::sdt::madt::{Madt, MadtEntry};
use core::pin::Pin;

use self::acpi_handler::SynapseAcpiHandler;

pub fn kernel_ap_entry() {
    // loop forever...
    loop {
        core::hint::spin_loop();
    }
}

pub fn init(cpu_config: &mut CpuConfig) {
    println!("Bootloader reports {} CPUs", cpu_config.get_num_cpus());

    let handler = SynapseAcpiHandler{};
    let acpi_tables = unsafe { acpi::AcpiTables::from_rsdp(handler, cpu_config.rsdp_address as usize).unwrap() };

    let madt_mapping = match acpi_tables.find_table::<Madt>() {
        Some(mapping) => mapping,
        None => panic!("System does not contain a Multiple APIC Description Table (MADT)!"),
    };

    // get a pinned reference to the madt table so we can iterate the local apic addresses
    let madt_ref = unsafe { madt_mapping.virtual_start.as_ref() };
    let madt = unsafe { Pin::new_unchecked(madt_ref) };

    let lapic_physical_address = madt.local_apic_address;
    println!("Local APIC base address: {:#X}", lapic_physical_address);

    // Record apic_id per CPU
    for entry in madt.entries() {
        match entry {
            // Type 0: Processor Local APIC (Standard 32-bit APIC systems)
            MadtEntry::LocalApic(lapic) => {
                // Always check the flags to make sure the processor is enabled or online-capable
                // Bit 0 = Enabled, Bit 1 = Online Capable
                let is_enabled = (lapic.flags & 0x1) != 0;
                let is_online_capable = (lapic.flags & 0x2) != 0;

                if is_enabled || is_online_capable {
                    println!(
                        "Found CPU -> Processor ID: {}, Local APIC ID: {}",
                        lapic.processor_id,
                        lapic.apic_id
                    );
                }
            }

            // Type 1: I/O APIC (TODO: store to assigning interrupts to CPUs?)
            MadtEntry::IoApic(io_apic) => {
                // ...
            }

            // Type 5: Local APIC Address Override
            MadtEntry::LocalApicAddressOverride(override_addr) => {
                let local_apic_address = override_addr.local_apic_address; // copy from packed/unaligned struct
                println!("Overriding Local APIC address to: {:#X}", local_apic_address);
            }

            // Type 9: Processor Local x2APIC (Modern 64-bit systems with >255 processors)
            MadtEntry::LocalX2Apic(x2apic) => {
                let is_enabled = (x2apic.flags & 0x1) != 0;
                let is_online_capable = (x2apic.flags & 0x2) != 0;
                let processor_uid = x2apic.processor_uid;
                let x2apic_id = x2apic.x2apic_id;

                if is_enabled || is_online_capable {
                    println!(
                        "Found modern x2APIC CPU -> ACPI Processor UID: {}, x2APIC ID: {}",
                        processor_uid,
                        x2apic_id
                    );
                }
            }

            // Ignore other entries (like NMI sources or Interrupt Source Overrides) for now
            _ => {}
        }
    }
}