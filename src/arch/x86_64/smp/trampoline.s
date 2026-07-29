.code16

.section .text
.globl trampoline_start
.globl trampoline_end

trampoline_start:
    cli                         # Disable interrupts
    xorw %ax, %ax
    movw %ax, %ds               # Set up clean segment registers
    movw %ax, %es
    movw %ax, %ss

    # 1. Load temporary 32-bit GDT (must use physical pointer addresses)
    # In real mode, lgdt needs an absolute 32-bit linear address
    lgdt entry_gdt_pointer

    # 2. Enable protected mode
    movl %cr0, %eax
    orl $1, %eax
    movl %eax, %cr0

    # 3. Far jump to 32-bit protected mode code segment (0x08)
    # This flushes the prefetch queue
    ljmp $0x08, protected_mode_entry

.code32
protected_mode_entry:
    movw $0x10, %ax            # Load 32-bit data segment
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss

    # 4. Enable PAE (Physical Address Extension) - Required for Long Mode
    movl %cr4, %eax
    orl $32, %eax              # Set PAE bit (1 << 5)
    movl %eax, %cr4

    # 5. Load your existing 64-bit CR3 Page Table Pointer
    # NOTE: You must write your active CR3 address to 'ap_cr3_target' from Rust
    movl ap_cr3_target, %eax
    movl %eax, %cr3

    # 6. Enable Long Mode in EFER MSR
    movl $0xC0000080, %ecx    # EFER MSR
    rdmsr
    orl $256, %eax            # Set LME (Long Mode Enable) bit (1 << 8)
    wrmsr

    # 7. Enable Paging to turn on Long Mode completely
    movl %cr0, %eax
    orl $0x80000000, %eax     # Set PG bit (1 << 31)
    movl %eax, %cr0

    # 8. Far jump into 64-bit mode (using your kernel's 64-bit Code Segment, e.g., 0x08 or 0x10)
    # NOTE: Replace 0x08 with your kernel's 64-bit code segment selector index
    ljmp $0x08, long_mode_entry

.code64
long_mode_entry:
    # 9. We are officially in 64-bit Long Mode!
    # Load your final 64-bit kernel data segment registers
    movw $0x10, %ax           # Replace with your 64-bit data segment selector
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movw %ax, %fs
    movw %ax, %gs

    # 10. Fetch unique stack pointer assigned by the BSP for this specific CPU core
    # Use RIP-relative 64-bit load to get the address
    movq ap_stack_target(%rip), %rax
    movq %rax, %rsp
    
    # 11. Call our Rust entrypoint
    movq ap_rust_entrypoint(%rip), %rax
    call *%rax

.halt_loop:
    hlt
    jmp .halt_loop


# --- Temporary Structures & Shared Variables Embedded in Trampoline ---
.balign 4
.section .data
entry_gdt:
    .quad 0x0000000000000000       # Null descriptor
    .quad 0x00cf9a000000ffff       # 32-bit Code segment (0x08)
    .quad 0x00cf92000000ffff       # 32-bit Data segment (0x10)
entry_gdt_end:

entry_gdt_pointer:
    .word entry_gdt_end - entry_gdt - 1
    .long 0                         # Will be filled dynamically by Rust at runtime

# Communication channels. Rust writes to these before sending SIPI.
.balign 8
.globl ap_cr3_target
.globl ap_stack_target
.globl ap_rust_entrypoint

ap_cr3_target:      .long 0         # 32-bit physical address of kernel CR3
ap_stack_target:     .quad 0         # 64-bit virtual address of allocated stack
ap_rust_entrypoint:  .quad 0         # 64-bit virtual address of kernel_ap_main

trampoline_end:
