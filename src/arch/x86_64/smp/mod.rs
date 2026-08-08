pub mod acpi_handler;
pub mod trampoline;

use x86_64::registers::model_specific::ApicBaseFlags;
use x86_64::structures::paging::PageTableFlags;
use satus_struct::cpu_config::{CpuConfig, PerCpuConfig};
use core::sync::atomic::{AtomicBool, Ordering};
use acpi::sdt::madt::{Madt, MadtEntry};
use core::pin::Pin;
use core::slice;
use core::ptr::{read_volatile, write_volatile};
use core::time::Duration;
use super::pager::get_kernel_cr3;
// TODO: should possibly use the pager's public physical_to_virtual API?
use crate::Address;
use crate::arch::x86_64::pager::PHYSICAL_OFFSET;
use crate::arch::x86_64::pager::set_mmio;
use crate::arch::x86_64::pit::timer_delay;
use crate::arch::x86_64::X86_PAGER;
use crate::arch::x86_64::pager::PhysicalAddress;
use crate::arch::x86_64::pager::VirtualAddress;
use crate::arch::x86_64::pager::PageType;
use crate::arch::x86_64::util::registers::get_cr3;
use self::acpi_handler::SynapseAcpiHandler;
use self::trampoline::Trampoline;

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

    // TODO: write this into cpu_config
    let mut lapic_physical_address: Address = madt.local_apic_address.into();
    println!("Local APIC base address: {:#X}", lapic_physical_address);

    let mmio_result = set_mmio(VirtualAddress(lapic_physical_address + PHYSICAL_OFFSET));
    println!("MMIO result {}", mmio_result);

    // TODO: need to enable apic?
    unsafe {
        core::arch::asm!(
            "mov $0x1B, %ecx",
            "rdmsr",
            "or $0xC00, %eax",     /* Set bit 11 (APIC Enable), bit 10 (x2apic enable) */
            "wrmsr",
            options(nostack, preserves_flags, att_syntax)
        );
    }
    /*
    mov $0x1B, %ecx
    rdmsr
    or $0x800, %eax     /* Set bit 11 (APIC Enable) */
    wrmsr
    */

    // TODO: disable caching on this page
    /**
    #define PAGE_PWT  (1 << 3) // Write-Through
    #define PAGE_PCD  (1 << 4) // Cache Disable

    pte[index] = PHYSICAL_LAPIC_BASE | PAGE_PRESENT | PAGE_WRITE | PAGE_PCD | PAGE_PWT;
    */

    println!("kerenl cr3 {:x} raw cr3 {:x}", get_kernel_cr3(), get_cr3());

    let trampoline = Trampoline::new(
        cpu_config.trampoline_address, 
        get_cr3(), 
        kernel_ap_entry as Address);
    
    X86_PAGER.get().unwrap().map_physical_to_virtual( 
        PhysicalAddress(cpu_config.trampoline_address),
        VirtualAddress(cpu_config.trampoline_address), 
        PageType::Page4KB,
        PageTableFlags::WRITABLE);

    let per_cpu_config = unsafe {
        slice::from_raw_parts_mut(
           cpu_config.per_cpu_config as *mut PerCpuConfig, 
            cpu_config.get_num_cpus() as usize)
    };

    // Record apic_id per CPU
    for entry in madt.entries() {
        let mut local_apid_id: Option<u32> = None;

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
                local_apid_id = Some(lapic.apic_id as u32);
            }

            // Type 1: I/O APIC (TODO: store to assigning interrupts to CPUs?)
            MadtEntry::IoApic(io_apic) => {
                // ...
            }

            // Type 5: Local APIC Address Override
            MadtEntry::LocalApicAddressOverride(override_addr) => {
                let local_apic_address = override_addr.local_apic_address; // copy from packed/unaligned struct
                println!("Overriding Local APIC address to: {:#X}", local_apic_address);
                lapic_physical_address = local_apic_address;
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
                local_apid_id = Some(x2apic_id);
            }

            // Ignore other entries (like NMI sources or Interrupt Source Overrides) for now
            _ => {}
        }

        if let Some(apic_id) = local_apid_id {
            let cpu_index = apic_id as usize;
            /** TODO
            // Store the APIC ID in the per_cpu_config for this CPU
            if cpu_index < per_cpu_config.len() {
                per_cpu_config[cpu_index].apic_id = apic_id;
            } else {
                println!("Warning: APIC ID {} exceeds the number of CPUs in the configuration", apic_id);
            }
            */
            unsafe {
                startup_ap(
                    apic_id, 
                    lapic_physical_address + PHYSICAL_OFFSET, 
                    &trampoline, 
                    &mut per_cpu_config[cpu_index]);
            }
        }
    }
}

unsafe fn startup_ap(
    apic_id: u32, 
    local_apic_base: Address,
    trampoline: &Trampoline, 
    per_cpu_config: &mut PerCpuConfig) {

    // skip the first one because we're already running?
    if apic_id == 0 {
        return;
    }
    
    println!("Starting up AP with Local APIC ID: {}", apic_id);

    let stack_pointer = per_cpu_config.stack.as_ptr() as Address;
    X86_PAGER.get().unwrap().show_address_debug(VirtualAddress(stack_pointer));

    for byte in per_cpu_config.stack.iter_mut() {
        *byte = 0x55;
    }
    trampoline.set_stack_pointer(stack_pointer);

    let mut x2apic_msr = x86_64::registers::model_specific::Msr::new(0x830);

    //let xapic2_result = x86_64::instructions::msr::rdmsr(/* IA32_APIC_BASE MSR */ 0x1B);
    //println!("xapic2 result: {}", xapic2_result);
    let (_, apic_flags) = x86_64::registers::model_specific::ApicBase::read();
    if apic_flags.contains(ApicBaseFlags::X2APIC_ENABLE) {
        println!("x2apid is enabled");
    }
    if apic_flags.contains(ApicBaseFlags::LAPIC_ENABLE) {
        println!("lapic is enabled");
    }
    println!("JJW");
    let x2apic = apic_flags.contains(ApicBaseFlags::X2APIC_ENABLE);

    // ensure apic is enabled (only if !x2apic?)
    let svr = local_apic_base + 0x0F0;
    let current_svr = read_volatile(svr as *const u32);
    // Bit 8 is Software Enable. Usually paired with a spurious vector (e.g., 0xFF)
    write_volatile(svr as *mut u32, current_svr | (1 << 8) | 0xFF);


    let icr_high = local_apic_base + 0x310; // ICR High register offset
    let icr_low = local_apic_base + 0x300;  // ICR Low register offset

    // Sent init IPI to the APIC ID
    let icr_high_value = apic_id << 24;
    let mut icr_low_value = 0x00004500; // INIT IPI, level-triggered
    if x2apic {
        let x2apic_icr_value = (apic_id as u64) << 32 | icr_low_value as u64;
        unsafe {
            x2apic_msr.write(x2apic_icr_value);
        }
    } else {
        write_volatile(icr_high as *mut u32, icr_high_value);
        write_volatile(icr_low as *mut u32, icr_low_value);
        let result = read_volatile(icr_low as *const u32);
        println!("Wrote {:x} {:x} to ICR result {:x}", icr_high_value, icr_low_value, result);
    }

    // delay 10ms
    timer_delay(Duration::from_millis(10));
    let result = read_volatile(icr_low as *const u32);
    println!("result {:x}", result);

    // Send Startup IPI (SIPI) to the APIC ID with the trampoline vector
    icr_low_value = 0x00004600 | (trampoline.get_vector() as u32); // SIPI with vector
    if x2apic {
        let x2apic_icr_value = (apic_id as u64) << 32 | icr_low_value as u64;
        unsafe {
            x2apic_msr.write(x2apic_icr_value);
        } 
    } else {
        write_volatile(icr_high as *mut u32, icr_high_value);
        write_volatile(icr_low as *mut u32, icr_low_value);
        let result = read_volatile(icr_low as *const u32);
        println!("Wrote {:x} {:x} to ICR result {:x}", icr_high_value, icr_low_value, result);
    }

    // delay 200us
    timer_delay(Duration::from_micros(200));
    let result = read_volatile(icr_low as *const u32);
    println!("result {:x}", result);

    // optional?

    // Send second SIPI to the APIC ID with the trampoline vector
    if x2apic {
        let x2apic_icr_value = (apic_id as u64) << 32 | icr_low_value as u64;
        unsafe {
            x2apic_msr.write(x2apic_icr_value);
        }  
    } else {
        write_volatile(icr_high as *mut u32, icr_high_value);
        write_volatile(icr_low as *mut u32, icr_low_value);
        println!("Wrote {:x} {:x} to ICR", icr_high_value, icr_low_value);
    }
}
/* 


#[repr(C)]
pub struct LocalApic {
    // ... other registers omitted for brevity ...
    icr_low: *mut u32,  // Offset 0x300
    icr_high: *mut u32, // Offset 0x310
}

impl LocalApic {
    pub unsafe fn boot_ap(&self, apic_id: u8, vector: u8) {
        // 1. Send INIT IPI
        // High register: Destination APIC ID shifted left by 24
        write_volatile(self.icr_high, (apic_id as u32) << 24);
        // Low register: Assert INIT, level-triggered (0x00004500)
        write_volatile(self.icr_low, 0x00004500);
        
        // TODO: Wait 10 milliseconds here
        
        // 2. Send First SIPI
        write_volatile(self.icr_high, (apic_id as u32) << 24);
        // Low register: Startup IPI, vector points to code (0x00004600 | vector)
        write_volatile(self.icr_low, 0x00004600 | (vector as u32));
        
        // TODO: Wait 200 microseconds here
        
        // 3. Send Second SIPI
        write_volatile(self.icr_high, (apic_id as u32) << 24);
        write_volatile(self.icr_low, 0x00004600 | (vector as u32));
    }
}
*/