pub mod acpi_handler;
pub mod trampoline;
pub mod relocation;
pub mod cpu_state;
pub mod cpu_stack;
pub mod per_cpu_data;

use x86_64::registers::model_specific::ApicBaseFlags;
use x86_64::structures::paging::PageTableFlags;
use satus_struct::cpu_config::{CpuConfig, PerCpuConfig};
use acpi::sdt::madt::{Madt, MadtEntry};
use core::pin::Pin;
use core::slice;
use core::ptr::{read_volatile, write_volatile};
use core::time::Duration;
use crate::Address;
use crate::arch::x86_64::pager::PHYSICAL_OFFSET;
use crate::arch::x86_64::pager::set_mmio;
use crate::arch::x86_64::pit::timer_delay;
use crate::arch::x86_64::X86_PAGER;
use crate::arch::x86_64::pager::PhysicalAddress;
use crate::arch::x86_64::pager::VirtualAddress;
use crate::arch::x86_64::pager::PageType;
use crate::arch::x86_64::util::registers::get_cr3;
use crate::types::CpuId;
use self::acpi_handler::SynapseAcpiHandler;
use self::trampoline::Trampoline;
use self::cpu_state::State;
use self::cpu_state::CpuState;
use self::cpu_stack::CpuStack;


pub fn kernel_ap_entry() {
    // Uncommenting this causes it to fail...
    let cpu_state = unsafe { CpuState::get_local_cpu_state() };
    cpu_state.state = cpu_state::State::Initializing;

    // loop forever...
    loop {
        core::hint::spin_loop();
    }
}

// REVISIT: a lot of this function (and others here) are very apic/x2apic related and, debateably, 
// should to contained within an apic/x2apic module and exposed to, and used by, this module.
// There's a lot of overlap between apic and smp which may deserve some refactoring once I get a 
// better idea of where the separation should be.
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

    // Not sure if we really need to do this, but... 
    // Try to enable APIC and X2APIC here
    unsafe {
        core::arch::asm!(
            "mov $0x1B, %ecx",
            "rdmsr",
            "or $0xC00, %eax",     /* Set bit 11 (APIC Enable), bit 10 (x2apic enable) */
            "wrmsr",
            options(nostack, preserves_flags, att_syntax)
        );
    }

    // Create our trampoline to take the APs from real mode to long mode
    let trampoline = Trampoline::new(
        cpu_config.trampoline_address,
        get_cr3(),
        kernel_ap_entry as *const () as Address,
    );

    // And identity map it (it must be within the 1st MB such that it's reachable in 16-bit real mode)
    let _ = X86_PAGER.get().unwrap().map_physical_to_virtual(
        PhysicalAddress(cpu_config.trampoline_address),
        VirtualAddress(cpu_config.trampoline_address),
        PageType::Page4KB,
        PageTableFlags::WRITABLE,
    );

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

            // Type 1: I/O APIC (TODO: store this to allow assigning interrupts to CPUs?)
            MadtEntry::IoApic(_io_apic) => {
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
            /* TODO
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

    let num_cpus = cpu_config.get_num_cpus();
    let mut num_aps_up = 1;
    while num_aps_up != num_cpus {
        num_aps_up = 1;
        for cpu in 1..num_cpus {
            let state = CpuState::get(cpu.try_into().unwrap());

            println!("Cpu {} state {}", cpu, state.state as u32);
            if state.state == State::Initializing {
                num_aps_up += 1;
            }
        }
    }
}

unsafe fn startup_ap(
    apic_id: u32, 
    local_apic_base: Address,
    trampoline: &Trampoline, 
    per_cpu_config: &mut PerCpuConfig) {

    // Skip the first one because we're already running (BSP)
    // Trying to start the CPU we're already running on can/will cause a fault. 
    if apic_id == 0 {
        return;
    }
    
    println!("Starting up AP with Local APIC ID: {}", apic_id);

    // Ensure apic is "Software enabled"
    let svr = local_apic_base + 0x0F0;
    let current_svr = unsafe { read_volatile(svr as *const u32) };
    // Bit 8 is Software Enable. Usually paired with a spurious vector (e.g., 0xFF)
    unsafe { write_volatile(svr as *mut u32, current_svr | (1 << 8) | 0xFF); }

    // TODO: REVISIT: this is a simple array allocated by the bootloader because it was 
    // easy to do so, but it differs in size and relative location compared to the BSP 
    // stack; probably there needs to be a unified strategy so that the AP stacks are comparatively 
    // similar to the BSP stack
    // This is also providing the top of the stack which is technically wrong.
    //let stack_pointer = per_cpu_config.stack.as_ptr() as Address;
    //X86_PAGER.get().unwrap().show_address_debug(VirtualAddress(stack_pointer));

    // TODO: populate
    let cpu_id = apic_id as CpuId;
    let cpu_state = CpuState::new(cpu_id);
    let cpu_stack = CpuStack::new(cpu_id);
    cpu_state.apic_id = u8::try_from(apic_id).unwrap();

    trampoline.set_stack_pointer(cpu_stack.base.0); // TODO: accept the `cpu_stack` itself?
    trampoline.set_cpu_state(cpu_state);

    let (_, apic_flags) = x86_64::registers::model_specific::ApicBase::read();
    let apic_enabled = apic_flags.contains(ApicBaseFlags::LAPIC_ENABLE);
    let x2apic_enabled = apic_flags.contains(ApicBaseFlags::X2APIC_ENABLE);

    if apic_enabled {
        println!("lapic is enabled");
    }
    if x2apic_enabled {
        println!("x2apid is enabled");
    }

    if x2apic_enabled {
        unsafe { startup_ap_x2apic(
            apic_id,
            local_apic_base,
            trampoline, 
            per_cpu_config); }
    } else {
        unsafe { startup_ap_lapic(
            apic_id,
            local_apic_base,
            trampoline, 
            per_cpu_config); }
    }
}

unsafe fn startup_ap_x2apic(
    apic_id: u32, 
    _local_apic_base: Address,
    trampoline: &Trampoline, 
    _per_cpu_config: &mut PerCpuConfig) {

    let mut x2apic_msr = x86_64::registers::model_specific::Msr::new(0x830);

    let mut icr_low_value = 0x00004500; // INIT IPI, level-triggered
    let x2apic_icr_value = (apic_id as u64) << 32 | icr_low_value as u64;
    unsafe {
        x2apic_msr.write(x2apic_icr_value);
    }

    // delay 10ms
    timer_delay(Duration::from_millis(10));
    // TODO: How to read status from x2apic?

    // Send Startup IPI (SIPI) to the APIC ID with the trampoline vector
    icr_low_value = 0x00004600 | (trampoline.get_vector() as u32); // SIPI with vector
    let x2apic_icr_value = (apic_id as u64) << 32 | icr_low_value as u64;
    unsafe {
        x2apic_msr.write(x2apic_icr_value);
    } 

    // delay 200us
    timer_delay(Duration::from_micros(200));
    // TODO: read status (skip second SIPI if not required)

    // Optional: Send second SIPI to the APIC ID with the trampoline vector
    let x2apic_icr_value = (apic_id as u64) << 32 | icr_low_value as u64;
    unsafe {
        x2apic_msr.write(x2apic_icr_value);
    }  
}

unsafe fn startup_ap_lapic(
    apic_id: u32, 
    local_apic_base: Address,
    trampoline: &Trampoline, 
    _per_cpu_config: &mut PerCpuConfig) {

    let icr_high = local_apic_base + 0x310; // ICR High register offset
    let icr_low = local_apic_base + 0x300;  // ICR Low register offset

    // Sent init IPI to the APIC ID
    let icr_high_value = apic_id << 24;
    let mut icr_low_value = 0x00004500; // INIT IPI, level-triggered
    unsafe { write_volatile(icr_high as *mut u32, icr_high_value); }
    unsafe { write_volatile(icr_low as *mut u32, icr_low_value); }
    let result = unsafe { read_volatile(icr_low as *const u32) };
    println!("Wrote {:x} {:x} to ICR result {:x}", icr_high_value, icr_low_value, result);

    // delay 10ms
    timer_delay(Duration::from_millis(10));
    let result = unsafe { read_volatile(icr_low as *const u32) };
    println!("result {:x}", result);

    // Send Startup IPI (SIPI) to the APIC ID with the trampoline vector
    icr_low_value = 0x00004600 | (trampoline.get_vector() as u32); // SIPI with vector
    unsafe { write_volatile(icr_high as *mut u32, icr_high_value); }
    unsafe { write_volatile(icr_low as *mut u32, icr_low_value); }
    let result = unsafe { read_volatile(icr_low as *const u32) };
    println!("Wrote {:x} {:x} to ICR result {:x}", icr_high_value, icr_low_value, result);

    // delay 200us
    timer_delay(Duration::from_micros(200));
    let result = unsafe { read_volatile(icr_low as *const u32) };
    println!("result {:x}", result);

    // optional?

    // Send second SIPI to the APIC ID with the trampoline vector
    unsafe { write_volatile(icr_high as *mut u32, icr_high_value); }
    unsafe { write_volatile(icr_low as *mut u32, icr_low_value); }
    println!("Wrote {:x} {:x} to ICR", icr_high_value, icr_low_value);
}