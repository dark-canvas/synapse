.code16
.intel_syntax noprefix
.section .text
.globl trampoline_start
.globl trampoline_end

trampoline_start:
    cli                         # Disable interrupts
    xor ax, ax
    mov ds, ax                  # Set up clean segment registers
    mov es, ax
    mov ss, ax

    # 1. Load temporary 32-bit GDT (must use physical pointer addresses)
    # In real mode, lgdt needs an absolute 32-bit linear address
    lgdt [entry_gdt_pointer]

    # 2. Enable protected mode
    mov eax, cr0
    or eax, 1
    mov cr0, eax

    # 3. Far jump to 32-bit protected mode code segment (0x08)
    # This flushes the prefetch queue
    jmp 0x08:protected_mode_entry

.code32
protected_mode_entry:
    mov ax, 0x10                # Load 32-bit data segment
    mov ds, ax
    mov es, ax
    mov ss, ax

    # 4. Enable PAE (Physical Address Extension) - Required for Long Mode
    mov eax, cr4
    or eax, 32                  # Set PAE bit (1 << 5)
    mov cr4, eax

    # 5. Load your existing 64-bit CR3 Page Table Pointer
    # NOTE: You must write your active CR3 address to 'ap_cr3_target' from Rust
    mov eax, [ap_cr3_target]
    mov cr3, eax

    # 6. Enable Long Mode in EFER MSR
    mov ecx, 0xC0000080         # EFER MSR
    rdmsr
    or eax, 256                 # Set LME (Long Mode Enable) bit (1 << 8)
    wrmsr

    # 7. Enable Paging to turn on Long Mode completely
    mov eax, cr0
    or eax, 0x80000000          # Set PG bit (1 << 31)
    mov cr0, eax

    # 8. Far jump into 64-bit mode (using your kernel's 64-bit Code Segment, e.g., 0x08 or 0x10)
    # NOTE: Replace 0x08 with your kernel's 64-bit code segment selector index
    jmp 0x08:long_mode_entry

.code64
long_mode_entry:
    # 9. We are officially in 64-bit Long Mode!
    # Load your final 64-bit kernel data segment registers
    mov ax, 0x10                # Replace with your 64-bit data segment selector
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov fs, ax
    mov gs, ax

    # 10. Fetch unique stack pointer assigned by the BSP for this specific CPU core
    # Use an intermediate register to load a 64-bit value from memory
    mov rax, [ap_stack_target]
    mov rsp, rax
    
    # 11. Call our Rust entrypoint
    mov rax, [ap_rust_entrypoint]
    call rax

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
