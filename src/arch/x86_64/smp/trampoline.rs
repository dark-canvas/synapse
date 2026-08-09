use core::ffi::c_void;
use core::arch::global_asm;
use crate::arch::x86_64::pager::PHYSICAL_OFFSET;
use crate::Address;

// This code will reside within the kernel (which has a linear base address of 0xFFFFFFFF80000000), 
// but the trampoline code will be copied down to a physical address below 1MB for the APs to execute,
// which means that we can't rely on the linker to resolve any addresses for us, for many reaosns:
// - Absolute addresses will resolve to within the kernel's linear address space, which is not where 
//   the trampoline code will be copied to.
// - Relative addresses (RIP-relative) could potentially work, but ip relative data addressing is not 
//   supported in protected mode (or real mode), and even if it was, there are various loads that must be 
//   absolute (eg. lgdt, mov cr3, etc).
// - We *could* remove some of these relations by using a a constant location for the trampoline and 
//   embedding the already linked/absolutely-positioned code, but we'd still need to patch in the values for 
//   CR3, the stack pointer and entry point.  Additionally, we don't have a way to guarantee a specific 
//   address for the trampoline because it's dynamically allocated by the bootloader via the UEFI firmware 
//   (if we just chose an address, there's no guarantee that the UEFI firmware may also try to use the 
//   same address for something else).
//
// For all of these reasons, I've opted to embed the trampoline code using immediate values in order to 
// avoid any reloactions, and will patch these values dynamically at runtime.
global_asm!(
    ".code16",
 
    ".section .trampoline",
    ".globl trampoline_start",
    ".globl trampoline_end",
    ".globl gdt_pointer",
    ".globl gdt_start",

    ".globl lgdt_patch",
    ".globl gdt_pointer_patch",
    ".globl cr3_patch",
    ".globl stack_patch",
    ".globl ap_entry_patch",
    ".globl projected_mode_entry_patch",
    ".globl long_mode_entry_patch",
 
    "trampoline_start:",
    "    cli",
    "    xorw %ax, %ax",
    "    movw %ax, %es",
    "    movw %ax, %ss",
    // ensure ds is set to the sipi vector cs
    "    movw %cs, %ax",
    "    movw %ax, %ds",

    "lgdt_patch:",
    "    lgdt (0x1122)", // placeholder for relocation

    // enable protected mode
    "    movl %cr0, %eax",
    "    orl $1, %eax",
    "    movl %eax, %cr0",

    // far jump into pmode code (clear prefetch queue)
    "    .byte 0x66",                  // operand size prefix for 32-bit
    "    .byte 0xea",                  // JMP FAR opcode
    "protected_mode_entry_patch:",
    "    .long 0x11223344",            // 32-bit address
    "    .word 0x0018",                // 16-bit selector
 
    ".code32",
    "protected_mode_entry:",
    "    movw $0x20, %ax",
    "    movw %ax, %ds",
    "    movw %ax, %es",
    "    movw %ax, %ss",

    // enable PAE (physical address extension) and PSE (page size extenstions)
    "    movl %cr4, %eax",
    "    orl $0x30, %eax",            // Set BOTH PAE (0x20) and PSE (0x10)
    "    movl %eax, %cr4",

    // load kernel's CR3 value (NOTE: it must be below 4GB since we're setting it in pmode)
    "cr3_patch:",
    "    movl $0x11223344, %eax", // placeholder for relocation
    "    movl %eax, %cr3",

    // enable long mode (alone with NXE (not executable extension)
    "    movl $0xC0000080, %ecx",
    "    rdmsr",
    "    orl $0x900, %eax",           // Set BOTH LME (0x100) and NXE (0x800)
    "    wrmsr",    

    // enable paging (required for long mode)
    "    movl %cr0, %eax",
    "    orl $0x80000000, %eax",
    "    movl %eax, %cr0",

    // far ump into long mode (clear prefetch queue)
    "    .byte 0xea",                  // JMP FAR opcode
    "long_mode_entry_patch:",
    "    .long 0x11223344",            // 32-bit offset
    "    .word 0x0008",                // 16-bit selector
 
    ".code64",
    "long_mode_entry:",
    "    movw $0x10, %ax",
    "    movw %ax, %ds", // delete?
    "    movw %ax, %es", // delete?
    "    movw %ax, %ss", // delete?
    "    movw %ax, %fs",
    "    movw %ax, %gs",

    // nops to ensure stack_patch is aligned (for relocation)
    "nop",
    "nop",
    "nop",
    "nop",
    "nop",
    "stack_patch:",
    "    movq $0x1122334455667788, %rax", // placeholder for relocation
    "    movq %rax, %rsp",

    // jump into the rust entry code
    "ap_entry_patch:",
    "    movq $0x1122334455667788, %rax", // placeholder for relocation
    "    call *%rax",
 
    ".halt_loop:",
    "    hlt",
    "    jmp .halt_loop",
 
    // no need to define this as .text... it's just a single contiguous block 
    // copied into the SIPI vector location
    // TODO/REVISIT: Have the APs load the exact same gdt as the BSP?
    ".balign 4",
    "gdt_start:",
    "    .quad 0x0000000000000000",
    "    .quad 0x00af9a000000ffff", // lmode code segment 0x08 (matches BSP selector)
    "    .quad 0x00af92000000ffff", // lmode data segment 0x10 (matches BSP selector)
    "    .quad 0x00cf9a000000ffff", // pmode code segment 0x18
    "    .quad 0x00cf92000000ffff", // pmode data segment 0x20
    "gdt_end:",
 
    "gdt_pointer:",
    "    .word gdt_end - gdt_start - 1",
    "gdt_pointer_patch:",
    "    .long 0",

    "trampoline_end:",
    options(att_syntax)
);

unsafe extern "C" {
    // NOTE: some of these probably need to be a u128, as they hold a 64-bit value and 
    // an instruction
    static mut lgdt_patch: u64;
    static mut gdt_pointer_patch: u32;
    static mut cr3_patch: u64;
    static mut stack_patch: u128;
    static mut ap_entry_patch: u128;
    static mut long_mode_entry_patch: u32;
    static mut protected_mode_entry_patch: u32;
    // bounds of the trampoline to copy
    static trampoline_start: Address;
    static trampoline_end: Address;
    static gdt_pointer: Address;
    static gdt_start: Address;
}

macro_rules! relocate_jump {
    ($jump:ident, $new_base:ident) => {
        let jump_offset = 
            Self::get_offset(unsafe { &raw const $jump as *const _ as Address }) as u32
            + 6; // jump past the offset + the selector
        $jump = $new_base as u32 + jump_offset;
    };
}

macro_rules! relocate {
    ($reloc:ident, $mask:expr, $value:ident) => {
        // TODO: assert that bits in expr == bits in value
        let bit_shift = $mask.trailing_zeros();
        let mut reloc_copy = $reloc;
        
        let mut typed_value = $mask;
        typed_value = $value.into();
        
        reloc_copy &= !$mask;
        reloc_copy |= (typed_value << bit_shift);
        $reloc = reloc_copy;
    };
}

pub struct Trampoline {
    address: Address,
}

impl Trampoline {

    fn get_offset(address: Address) -> usize {
        let base_address = unsafe { &trampoline_start as *const _ as usize };
        let target_address = address as usize;
        target_address - base_address
    }

    // TODO: specify the entry point as a rust function, and possibly even 
    // allow for passing a parameter to it (or a pointer to a struct of parameters)
    pub fn new(address: Address, cr3: Address, entry_point: u64) -> Self {
        assert!(address < 1024 * 1024); // 1mb
        assert!(address % 4096 == 0); // page aligned

        let cr3 = u32::try_from(cr3).expect("CR3 address must be below 4GB");

        println!("Patching trampoline {:#x} -> {:#x}",
            unsafe { &trampoline_start as *const _ as usize },
            address
        );

        println!("ap entry: {:#x}", entry_point);

        // update any static values before we copy...
        unsafe {
            // patch up the jumps... the patch points to the address part of the jump
            // instruction (after the jump/0xea opcode), and the address needs to be 
            // updated to jump immediately after the instruction (after the address, 
            // which includes a selector as well)
            relocate_jump!(protected_mode_entry_patch, address);
            relocate_jump!(long_mode_entry_patch, address);

            let relocated_gdt_pointer = 
                Self::get_offset(unsafe { &gdt_pointer as *const _ as Address }) as u64;
            let gdt_start_offset = 
                Self::get_offset(unsafe { &gdt_start as *const _ as Address }) as u64;
            let relocated_gdt_start = 
                gdt_start_offset as u32 + address as u32;

            relocate!(cr3_patch, 0xffffffff00_u64, cr3);
            relocate!(ap_entry_patch, 0xffffffffffffffff0000_u128, entry_point);
            relocate!(lgdt_patch, 0xffff000000_u64, relocated_gdt_pointer);
            relocate!(gdt_pointer_patch, 0xffffffff_u32, relocated_gdt_start);
        }

        // then copy into the real-mode address provided...
        let start = unsafe { &trampoline_start as *const _ as *const c_void };
        let end = unsafe { &trampoline_end as *const _ as *const c_void };
        let size = (end as usize) - (start as usize);
        let trampoline_ptr = (address + PHYSICAL_OFFSET) as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(start as *const u8, trampoline_ptr, size);
        }

        Self {
            address: address
        }
    }

    pub fn get_vector(&self) -> Address {
        self.address / 4096
    }

    pub fn set_stack_pointer(&self, stack_pointer: Address) {
        println!("set_stack_pointer: {:#x}", stack_pointer);
        // stack_patch points to the original trampoline, but we need the relocated address
        let stack_patch_orig = unsafe { stack_patch };
        
        let stack_patch_offset = Self::get_offset(unsafe { &raw const stack_patch as *const _ as Address }) as u64;
        let relocated_stack_patch = 
            self.address + PHYSICAL_OFFSET + stack_patch_offset;
        let stack_patch_ptr = relocated_stack_patch as *mut u128;
        
        unsafe {
            // movq $0x1122334455667788, %rax" == 0x48 (rex.w prefix), 0xb8, 64-bit address
            assert!(stack_patch & 0x0000000000ffffffffffffffffffff == 0x1122334455667788b848u128);
            let mut stack_patch_copy: u128 = *stack_patch_ptr & 0xffffffffffff0000000000000000ffff;
            println!("stack patch orig: {:#x}", stack_patch_orig);
            println!("stack patch copy: {:#x}", stack_patch_copy);
            stack_patch_copy |= (stack_pointer as u128) << 16;
            println!("stack patch done: {:#x}", stack_patch_copy);
            *stack_patch_ptr = stack_patch_copy;
        }
    }
}