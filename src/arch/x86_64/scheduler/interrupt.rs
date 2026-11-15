use core::time::Duration;
use x86_64::structures::idt::InterruptStackFrame;
use crate::arch::x86_64::scheduler::{TASK_MAP_BASE_ADDRESS, Task};
use crate::arch::x86_64::x2apic;
use crate::arch::x86_64::smp::CpuState;
use crate::scheduler::Scheduler;

#[unsafe(naked)]
pub extern "x86-interrupt" fn timer_interrupt_handler(stack_frame: InterruptStackFrame)
{
    /* The stack constructed upon this call from ring 3::
     * rsp + 32: return stack selector (padded to 8 bytes),
     * rsp + 24: return rsp
     * rsp + 16: return rflags
     * rsp +  8: return code segment
     * rsp +  0: return instruction pointer
     * 
     * If the timer interrupt is called from ring 0, RSP and SS are not included.
     * Inspecting the RPL of CS can determine whether RSP and SS are there.
     */
    core::arch::naked_asm!(
        "    push %rdi",

        // calculate the current task pointer by querying the current taskID from the cpu state, 
        // multiply by the size of a Task, and add the task map base address to it.
        "    xorq %rdi, %rdi",
        "    movl %gs:{CURRENT_TASK_OFFSET}, %edi",
        "    subl $1, %edi",                   // subtract 1 to get the task index
        "    imulq ${TASK_SIZE}, %rdi, %rdi",
        "    push %rax",
        "    movq ${TASK_MAP_BASE}, %rax",
        "    addq %rax, %rdi",                 // add the base of the task map
        "    pop %rax",

        // save all the x86-64 ABI clobbered/scratch registers first,
        // note rdi and rflags pushed on the stack
        "    movq %rax,   0(%rdi)",
        "    movq %rbx,   8(%rdi)",
        "    movq %rcx,  16(%rdi)",
        "    movq %rdx,  24(%rdi)",
        "    movq %rsi,  32(%rdi)",
        "    movq %rbp,  48(%rdi)",
        "    movq %r8,   64(%rdi)",
        "    movq %r9,   72(%rdi)",
        "    movq %r10,  80(%rdi)",
        "    movq %r11,  88(%rdi)",
        "    movq %r12,  96(%rdi)",
        "    movq %r13, 104(%rdi)",
        "    movq %r14, 112(%rdi)",
        "    movq %r15, 120(%rdi)",

        // handle rdi
        "    pop %rax",
        "    movq %rax, 40(%rdi)", // RDI

        // pop off the return stack and save it off
        "    pop %rax",  // rip
        "    movq %rax, 128(%rdi)",
        "    pop %rbx",  // cs
        "    movw %bx, {CS_OFFSET}(%rdi)",
        "    pop %rax",  // rflags
        "    movq %rax, 136(%rdi)",

        // if cs is not ring 0, then pop off the userland rsp and ss and save them off
        "    andq $0x3, %rbx",
        "    cmpq $0x0, %rbx",
        "    je 1f",
        "    pop %rax",  // rsp
        "    movq %rax, 56(%rdi)",
        "    pop %rax",  // ss
        "    movw %ax, {SS_OFFSET}(%rdi)",
        "    jmp 2f",
        
        "1:",
        // if cs *is* ring 0, then rsp and ss needs to be saved off another way
        // we've already popped off the return stack, so rsp is where we'd want to save it
        "    movq %rsp, 56(%rdi)",
        "    movw ${RING0_SS}, {SS_OFFSET}(%rdi)",
        "2:",
        "    callq timer_interrupt_handler_rust",

        // restore state from pending task (which rust code indicated)

        "    xorq %rdi, %rdi",
        "    movl %gs:{CURRENT_TASK_OFFSET}, %edi",  // TODO: pass in offset_of as a const
        "    subl $1, %edi",             // subtract 1 to get the task index
        "    imulq ${TASK_SIZE}, %rdi, %rdi",  // immediate value is size_of::<Task>()
        "    movq ${TASK_MAP_BASE}, %rax",
        "    addq %rax, %rdi",           // add the base of the task map

        // depending on how we got here, we might be using the tss's rsp0, 
        // or we might be using the current task's rsp.
        // we can continue to use rsp0 to return to either ring0 or ring3, since we've configured
        // the size or rsp0 ourselves and know there's enough room to push the return stack and ss/rsp if needed.
        // If we're using a ring0 stack, then we know there's enough stack to return to another ring0 task, but 
        // there (technically...) may not be enough to return to a ring3 task... ??

        // if cs is not ring 0, we need to push ss and rsp first
        // TODO: is cs is ring 0, we still need to restore rsp and ss... where?
        // unconditionally restore rsp here?
        "    xorq %rcx, %rcx",
        "    movw {CS_OFFSET}(%rdi), %bx",
        "    movw %bx, %cx",
        "    andw $0x3, %bx",
        "    cmpw $0x0, %bx",
        "    jne 3f",
        // returning to ring 0, we we need to switch to that stack first
        "    movq 56(%rdi), %rsp",
        // TODO: REVISIT: should we also set ss here?
        "    jmp 4f",

        "3:",  // returning to ring 3, so push ss and rsp first
        "    xorq %rax, %rax",
        "    movw {SS_OFFSET}(%rdi), %ax",
        "    push %rax",
        "    movq 56(%rdi), %rax",
        "    push %rax",

        "4:", // common to both ring 0 and ring 3 return paths, push the rest of the return stack
        "    movq 136(%rdi), %rax",
        "    push %rax",
        "    push %rcx", // we already have the cs in rcx
        "    movq 128(%rdi), %rax",
        "    push %rax",

        "    movq  0(%rdi), %rax",
        "    movq  8(%rdi), %rbx",
        "    movq 16(%rdi), %rcx",
        "    movq 24(%rdi), %rdx",
        "    movq 32(%rdi), %rsi",
        "    movq 48(%rdi), %rbp",
        "    movq 64(%rdi), %r8",
        "    movq 72(%rdi), %r9",
        "    movq 80(%rdi), %r10",
        "    movq 88(%rdi), %r11",       
        "    movq 96(%rdi), %r12",
        "    movq 104(%rdi), %r13",
        "    movq 112(%rdi), %r14",
        "    movq 120(%rdi), %r15",
        "    movq  40(%rdi), %rdi",

        "    iretq",
        TASK_SIZE = const core::mem::size_of::<Task>(),
        TASK_MAP_BASE = const TASK_MAP_BASE_ADDRESS.0,
        CURRENT_TASK_OFFSET = const core::mem::offset_of!(CpuState, current_tid),
        RING0_SS = const (crate::arch::x86_64::gdt::RING_0_DATA_SELECTOR | 0),
        CS_OFFSET = const core::mem::offset_of!(Task, cs),
        SS_OFFSET = const core::mem::offset_of!(Task, ss),
        options(att_syntax)
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn timer_interrupt_handler_rust() {

    let cpu_state = CpuState::get_local_cpu_state();
    let scheduler = &mut cpu_state.scheduler;

    // call common scheduler code
    scheduler.context_switch();

    unsafe {
        cpu_state.lapic.end_of_interrupt();
    }

    // re-enable the timer...
    x2apic::timer::timer_set(Duration::from_millis(10));
}
