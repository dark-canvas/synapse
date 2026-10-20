/*
At the kernel level, you program the PIT (Programmable Interval Timer) or APIC timer by writing to specific hardware I/O ports. The x86_64 crate is used to handle interrupts and register setup. 

    Timer Frequency: The PIT operates at a base frequency of
    . To set a specific interval, you divide this base frequency by the desired frequency (Hz).
    Implementation:
        Disable Interrupts: Use x86_64::instructions::interrupts::disable() to prevent conflicts during configuration.
        Configure PIT: Send the divisor to I/O port 0x40 after sending control word 0x36 to port 0x43.
        Setup Handler: Register an extern "x86-interrupt" function in the Interrupt Descriptor Table (IDT) for the timer interrupt.
        Enable Interrupts: Use x86_64::instructions::interrupts::enable(). 
*/

use core::arch::global_asm;
use core::time::Duration;
use x86_64::structures::idt::InterruptStackFrame;
use x86_64::registers::model_specific::Msr;
use crate::arch::x86_64::global_data::GlobalData;
use crate::arch::x86_64::scheduler::{TASK_MAP_BASE_ADDRESS, Task};
use crate::arch::x86_64::x2apic::timer::timer_set;
use crate::arch::x86_64::scheduler::scheduler_get_next_task;

use crate::arch::x86_64::smp::CpuState;
use crate::scheduler::TaskMap as _;

// Port delcarations
#[allow(dead_code)]
const TIMER0_CNTR: u16 = 0x40;
#[allow(dead_code)]
const TIMER1_CNTR: u16 = 0x41;
#[allow(dead_code)]
const TIMER2_CNTR: u16 = 0x42;
#[allow(dead_code)]
const TIMER_CTRL: u16 = 0x43;

const TIMER2_GATE: u16 = 0x61;

// for use with TIMER_CTRL port
#[allow(dead_code)]
const TIMER0: u8 = 0 << 6;
#[allow(dead_code)]
const TIMER1: u8 = 1 << 6;
#[allow(dead_code)]
const TIMER2: u8 = 2 << 6;

#[allow(dead_code)]
const TIMER_LATCH: u8 = 0 << 4;
#[allow(dead_code)]
const TIMER_LSB: u8 = 1 << 4;
#[allow(dead_code)]
const TIMER_MSB: u8 = 2 << 4;
#[allow(dead_code)]
const TIMER_WHOLE: u8 = TIMER_LSB | TIMER_MSB;

#[allow(dead_code)]
const TIMER_MODE0: u8 = 0 << 1;
#[allow(dead_code)]
const TIMER_MODE1: u8 = 1 << 1;
#[allow(dead_code)]
const TIMER_MODE2: u8 = 2 << 1;
#[allow(dead_code)]
const TIMER_MODE3: u8 = 3 << 1;
#[allow(dead_code)]
const TIMER_MODE4: u8 = 4 << 1;
#[allow(dead_code)]
const TIMER_MODE5: u8 = 5 << 1;

const TIMER_BIN16: u8 = 0;
#[allow(dead_code)]
const TIMER_BCD: u8 = 1;

const PIT_FREQ: u32 = 0x1234DE;

const MICROS_PER_SECOND: u64 = 1_000_000;

static mut TICKS: u64 = 0;
static mut FREQ: u64 = 0;

unsafe extern "C" {
    /// Voluntarily called by a task to yield control to the scheduler, which will select the next task to run.
    pub fn timer_interrupt_asm();
}

// Inline assembly function directly into the binary
// It's important to note the convention that's used when calling this method:
//   The call instruction: It decrements the Stack Pointer (RSP) by 8 bytes, 
//   copies the address of the next sequential instruction (the return address) into that new memory location, 
//   and then jumps to the target function.
global_asm!(
    ".global timer_interrupt_asm",
    ".text",
    "timer_interrupt_asm:",
    "iretd"
);


#[unsafe(naked)]
pub extern "x86-interrupt" fn timer_interrupt_handler(stack_frame: InterruptStackFrame)
{
    /* Stack is:
     * rsp + 32: return stack selector (padded to 8 bytes),
     * rsp + 24: return rsp
     * rsp + 16: return rflags
     * rsp +  8: return code segment
     * rsp +  0: return instruction pointer
     */
    core::arch::naked_asm!(
        "    push %rdi",

        // calculate the current task pointer by querying the current taskID from the cpu state, 
        // multiply by the size of a Task, and add the task map base address to it.
        "    xorq %rdi, %rdi",
        "    movl %gs:{CURRENT_TASK_OFFSET}, %edi",  // TODO: pass in offset_of as a const
        "    subl $1, %edi",             // subtract 1 to get the task index
        "    imulq ${TASK_SIZE}, %rdi, %rdi",  // immediate value is size_of::<Task>()
        "    push %rax",
        "    movq ${TASK_MAP_BASE}, %rax",
        "    addq %rax, %rdi",           // add the base of the task map
        "    pop %rax",

        // save all the x86-64 ABI clobbered/scratch registers first,
        // note rdi and rflags pushed on the stack
        "    movq %rax,  0(%rdi)",
        "    movq %rbx, 8(%rdi)",
        "    movq %rcx, 16(%rdi)",
        "    movq %rdx, 24(%rdi)",
        "    movq %rsi, 32(%rdi)",
        "    movq %rbp, 48(%rdi)",
        "    movq %r8,  64(%rdi)",
        "    movq %r9,  72(%rdi)",
        "    movq %r10, 80(%rdi)",
        "    movq %r11, 88(%rdi)",
        "    movq %r12, 96(%rdi)",
        "    movq %r13, 104(%rdi)",
        "    movq %r14, 112(%rdi)",
        "    movq %r15, 120(%rdi)",

        // handle rdi
        "    pop %rax",
        "    movq %rax, 40(%rdi)", // RDI

        // pop off the return stack and save it off
        "    pop %rax",  // rip
        "    movq %rax, 128(%rdi)",
        "    pop %rax",  // cs
        "    movw %ax, {CS_OFFSET}(%rdi)",
        "    pop %rax",  // rflags
        "    movq %rax, 136(%rdi)",
        "    pop %rax",  // rsp
        "    movq %rax, 56(%rdi)",
        "    pop %rax",  // ss
        "    movw %ax, {SS_OFFSET}(%rdi)",
        
        "    callq timer_rust",

        // restore state from pending task (which rust code indicated)

        "    xorq %rdi, %rdi",
        "    movl %gs:{CURRENT_TASK_OFFSET}, %edi",  // TODO: pass in offset_of as a const
        "    subl $1, %edi",             // subtract 1 to get the task index
        "    imulq ${TASK_SIZE}, %rdi, %rdi",  // immediate value is size_of::<Task>()
        "    movq ${TASK_MAP_BASE}, %rax",
        "    addq %rax, %rdi",           // add the base of the task map

        "    xorq %rax, %rax",
        "    movw {SS_OFFSET}(%rdi), %ax",
        "    push %rax",
        "    movq 56(%rdi), %rax",
        "    push %rax",
        "    movq 136(%rdi), %rax",
        "    push %rax",
        "    xorq %rax, %rax",
        "    movw {CS_OFFSET}(%rdi), %ax",
        "    push %rax",
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
        CS_OFFSET = const core::mem::offset_of!(Task, cs),
        SS_OFFSET = const core::mem::offset_of!(Task, ss),
        options(att_syntax)
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn timer_rust() {

    // call common scheduler code (TODO: find a way to make this common, and into move out of 
    // the arch folder)
    scheduler_get_next_task();

    unsafe {
        CpuState::get_local_cpu_state().lapic.end_of_interrupt();
    }

    // re-enable the timer...
    timer_set(Duration::from_millis(10));
}

fn timer_set_frequency(freq: u32) {
    let divisor: u16 = (PIT_FREQ / freq) as u16;
    unsafe {
        x86_64::instructions::port::Port::new(TIMER_CTRL).write(TIMER0 | TIMER_WHOLE | TIMER_MODE2 | TIMER_BIN16);
        x86_64::instructions::port::Port::new(TIMER0_CNTR).write((divisor & 0xFF) as u8); // LSB
        x86_64::instructions::port::Port::new(TIMER0_CNTR).write((divisor >> 8) as u8); // MSB
        FREQ = freq as u64;
    }
}

pub fn timer_get_ticks() -> u64 {
    unsafe { core::ptr::read_volatile(&raw const TICKS as *const u64) }
}

pub fn timer_delay(duration: Duration) {
    // The tick countdown is a u16, and the frequency is ~
    // Which means the maximum timer we can configure (without looping) is 54,925.4 µs
    assert!(duration.as_micros() < 54925);

    let mut pit_ticks: u16 = (PIT_FREQ as u64 * duration.as_micros() as u64 / 1_000_000) as u16;
    if pit_ticks == 0 {
        pit_ticks = 1;
    }

    unsafe {      
        // Configure PIT Channel 2: 
        //    Mode 0 (Interrupt on Terminal Count) 
        //    Access mode: lobyte/hibyte.
        //    BCD mode: binary
        x86_64::instructions::port::Port::new(TIMER_CTRL).write(
            TIMER2 | TIMER_WHOLE | TIMER_MODE0);

        let mut timer2_counter = x86_64::instructions::port::Port::new(TIMER2_CNTR);
        let mut timer2_gate = x86_64::instructions::port::Port::new(TIMER2_GATE);

        // write the count down ticks, lo byte then high byte
        timer2_counter.write((pit_ticks & 0xff) as u8);
        timer2_counter.write(((pit_ticks >> 8) & 0xFF) as u8);


        // Reset PIT Channel 2 gate, clearing the lower two bits
        //   Bit 0 = gate    => stop countdown
        //   Bit 1 = speaker => disable the speaker
        let mut pit_gate: u8 = timer2_gate.read(); //inb(0x61);
        pit_gate &= 0xFC;
        timer2_gate.write(pit_gate);
        timer2_gate.write(pit_gate | 0x01); // enable the countdown, keep speaker disabled

        // Wait for PIT Channel 2 to finish counting down.
        //    When Mode 0 finishes, the OUT pin (Bit 5 of port 0x61) goes HIGH.
        while (timer2_gate.read() & 0x20) == 0 {
            core::hint::spin_loop();
        }
    }
}

pub fn init() {
    timer_set_frequency(200); // 200Hz, arbitrary
}