use core::arch::asm;

use x86_64::structures::paging::PageTableFlags;

use crate::{arch::x86_64::{X86_PAGER, gdt::{RING_3_CODE_SELECTOR, RING_3_DATA_SELECTOR}, pager::PageType, smp}, pager::{PhysicalAddress, VirtualAddress}};

// STAR = system target address register

// In Long Mode, userland CS will be loaded from STAR 63:48 + 16 and userland SS from STAR 63:48 + 8 on SYSRET. You may need to modify your GDT accordingly.
const STAR : u32 = 0xC0000081;
/*
• Function: Holds the segment selectors used when transitioning between user space (ring 3) and kernel space (ring 0).
• Layout:
	• Bits 47–32: Kernel Code Segment (CS) and Stack Segment (SS) selectors loaded on SYSCALL.
	• Bits 63–48: User Return Code Segment (CS) and Stack Segment (SS) selectors loaded on SYSRET (adjusted automatically with RPL 3 for user mode).
	• Bits 31–0: Legacy target EIP for 32-bit SYSENTER/SYSCALL (largely unused/ignored in 64-bit long mode).
*/


const LSTAR : u32 = 0xC0000082; // The kernel's RIP SYSCALL entry for 64 bit software.


const CSTAR : u32 = 0xC0000083; // The kernel's RIP for SYSCALL in compatibility mode.


const SFMASK : u32 = 0xC0000084; // The low 32 bits are the SYSCALL flag mask. If a bit in this is set, the corresponding bit in rFLAGS is cleared.

const IA32_EFER	: u32 = 0xC0000080;
/*
• Function: The Flag Mask Register.
• Layout: The bits set in the lower 32 bits of IA32_FMASK are automatically cleared in the RFLAGS register when SYSCALL executes, allowing the kernel to start running with a known, clean CPU flag state (such as disabling interrupts via the IF flag). 
*/

extern "C" fn print_something() {
    println!("Hello from syscall handlers");
    //let star = unsafe { x86_64::registers::model_specific::Msr::new(0xC000_0081).read() };
    //println!("STAR={:x}", star);
    //println!("Done");
}

#[unsafe(naked)]
extern "C" fn syscall_handler()
{
    // Upon entry:
    //   rax = syscall number
    //   rdi, rsi, rdx, r10, r8, r9 = arguments
    //   rcx = saved userland RIP
    //   r11 = saved userland RFLAGS
    //   rsp = still pointing at userland stack/RSP
    core::arch::naked_asm!(
        // TODO: convert rax into a pointer and jump to it
        "push %rcx",
        "push %r11",
        "callq {HANDLER}",
        "popq %r11",
        "popq %rcx",
        "sysretq",

        HANDLER = sym print_something,
        options(att_syntax)
    );
}

pub fn perform_syscall_test() {
    unsafe {
        asm!(
            "jmp *{target}",
            target = in(reg) 0x400000usize,
            options(att_syntax, noreturn)
        )
    }
}

// this will get copied to 0x400000 which is called from perform_syscall_test() 
// above
pub extern "C" fn syscall_test_task() {
    unsafe {
        asm!(
            // drop to ring 3
            "cli",
            "pushq ${USER_SS}",
            //"pushq {USER_RSP}",
            //"pushq %rsp", // use the same stack for now
            "pushq $0x400800", // code is copied to 4mb, place the stack midway through that page
            "pushq $0x202", // rflags
            //"pushq $0x2", // rflags with interrupts disabled
            "pushq ${USER_CS}",
            //"pushq 1f",
            "leaq 1f(%rip), %rax",
            "pushq %rax",
            "iretq",
            "1:",
            // invoke syscall w/ id == 0
            "mov $0, %eax",
            "syscall",
            "jmp 1b",
            USER_SS = const (RING_3_DATA_SELECTOR | 3),
            USER_CS = const (RING_3_CODE_SELECTOR | 3),

            options(att_syntax)
        )
    }
}


pub fn init() {

    let mut efer = x86_64::registers::model_specific::Msr::new(IA32_EFER);
    unsafe {
        let mut value = efer.read();
        value |= 1;
        efer.write(value);
    }

    let mut syscall_entry_msr = x86_64::registers::model_specific::Msr::new(LSTAR);
    let syscall_entry : u64 = syscall_handler as usize as u64;
    unsafe {
        syscall_entry_msr.write(syscall_entry);
    }

    let mut syscall_flag_mask_msr = x86_64::registers::model_specific::Msr::new(SFMASK);
    unsafe {
        syscall_flag_mask_msr.write(0x200); // clear IF on syscall entry
    }

    // sysret  base (bits 63:48) = 0x18
    //   therefore:
    //     - user CS = 0x18 + 16 = 0x28
    //     - user SS = 0x18 +  8 = 0x20
    // syscall base (bits 47:32) = 0x8
    //   therefore:
    //     - kernel CS = 0x8
    //     - kernel SS = 0x8 + 8 = 0x10
    let sysret_base = 0x18;
    let syscall_base = 0x8;
    let mut syscall_selectors_msg = x86_64::registers::model_specific::Msr::new(STAR);
    let selector_bases : u64 = 
        (syscall_base << 32) +
        (sysret_base << 48);

    unsafe {
        syscall_selectors_msg.write(selector_bases);
    }

    //let star = unsafe { x86_64::registers::model_specific::Msr::new(0xC000_0081).read() };
    //println!("STAR={:#018x}", star);

    // The rest of this we only want to do once...
    let cpu_id = smp::get_cpu_id().unwrap();
    if cpu_id != 0 {
        println!("Skipping syscall copy on cpu {}", cpu_id);
        return;
    }

    println!("Copying syscall test task to 0x400000");

    // copy the syscall task into a userspace accessible memory region
    let syscall_task_addr = syscall_test_task as usize;
    let target_address = 4*1024*1024;

    let pager = X86_PAGER.get().unwrap();
    
    let page = pager.allocate_4kb_page().unwrap();
    pager.map_physical_to_virtual(
        PhysicalAddress(page), 
        VirtualAddress(target_address), 
        PageType::Page4KB, 
        PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE).unwrap();
    
    // clear the page for debug purposes
    unsafe { core::ptr::write_bytes(target_address as *mut u8, 0x0, 4096); }

    // TODO: need to find exactly how long this is... using 4096 bytes overwrites the zero'ing above
    let syscall_stask_size = 1024;

    let syscall_task_slice = unsafe { core::slice::from_raw_parts(syscall_task_addr as *const u8, syscall_stask_size) };
    let target_slice = unsafe { core::slice::from_raw_parts_mut(target_address as *mut u8, syscall_stask_size) };
    target_slice.copy_from_slice(syscall_task_slice);
}