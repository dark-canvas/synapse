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

use core::arch::asm;
use core::time::Duration;
use x86_64::VirtAddr;
use x86_64::structures::idt::InterruptStackFrame;
use x86_64::registers::model_specific::Msr;
use x86_64::registers::rflags::RFlags;
use super::scheduler::task::Task;
use super::scheduler::CURRENT_TASK;
use super::util::register_snapshot::RegisterSnapshot;
use crate::get_register_snapshot;
use crate::restore_register_snapshot;

// Port delcarations
const TIMER0_CNTR: u16 = 0x40;
const TIMER1_CNTR: u16 = 0x41;
const TIMER2_CNTR: u16 = 0x42;
const TIMER_CTRL: u16 = 0x43;

// for use with TIMER_CTRL port
const TIMER0: u8 = 0 << 6;
const TIMER1: u8 = 1 << 6;
const TIMER2: u8 = 2 << 6;

const TIMER_LATCH: u8 = 0 << 4;
const TIMER_LSB: u8 = 1 << 4;
const TIMER_MSB: u8 = 2 << 4;
const TIMER_WHOLE: u8 = TIMER_LSB | TIMER_MSB;

const TIMER_MODE0: u8 = 0 << 1;
const TIMER_MODE1: u8 = 1 << 1;
const TIMER_MODE2: u8 = 2 << 1;
const TIMER_MODE3: u8 = 3 << 1;
const TIMER_MODE4: u8 = 4 << 1;
const TIMER_MODE5: u8 = 5 << 1;

const TIMER_BIN16: u8 = 0;
const TIMER_BCD: u8 = 1;

const PIT_FREQ: u32 = 0x1234DD;

const MICROS_PER_SECOND: u64 = 1_000_000;

static mut TICKS: u64 = 0;
static mut FREQ: u64 = 0;

// It's debateable whether this belongs here, or in the IDT module...  ultimately it'll defer to common multitasking code...
pub extern "x86-interrupt" fn timer_interrupt_handler(
    stack_frame: InterruptStackFrame)
{
    // increment ticks by 1
    unsafe { TICKS += 1; }
    // Safe off the current task's state so that we can resume it later
    /*
    unsafe {
        (*CURRENT_TASK).registers = get_register_snapshot!();
        (*CURRENT_TASK).registers.rip = stack_frame.instruction_pointer.as_u64();
        (*CURRENT_TASK).registers.rflags = stack_frame.cpu_flags.bits();
    }
    */

    // Prior to executing this interrupt haandler, the processor would have pushed the following onto the stack:
    // 1. The current instruction pointer (RIP) and code segment (CS)
    // 2. The RFLAGS register
    // 3. The current stack pointer (RSP) and stack segment (SS)
    // TODO: Determine order?  I think it's reversed from above (https://alamot.github.io/os_tasking/)
    // Before we do anything, save off the location of these so that we can:
    //   store them in our task structure
    //   replace them with the next task to run
    
    /**
    For multitasking...
    1. Save the current task's state (registers, stack pointer, etc.) to its Task Control Block (TCB).
    2. Select the next task to run using a scheduling algorithm (e.g., round-robin, priority-based).
    3. Load the next task's state from its TCB, including its stack pointer and registers.
    4. Send an End of Interrupt (EOI) signal to the PIC or APIC to indicate that the interrupt has been handled.
    */
    
    // TODO: need a better/common way to signal EOI (for all interrupts) and in the x2apic mod
    unsafe { 
        let mut apic_eoi_msr = x86_64::registers::model_specific::Msr::new(0x80b);
        apic_eoi_msr.write(0x0)
    };

    // TODO: this doesn't work...
    /*
    unsafe {
        // restore registers and return to the "next" task (it's still the same one for now)
        let mut next_stack_frame = InterruptStackFrame::new(
            VirtAddr::new((*CURRENT_TASK).registers.rip),
            stack_frame.code_segment,
            RFlags::from_bits_truncate((*CURRENT_TASK).registers.rflags),
            VirtAddr::new((*CURRENT_TASK).registers.rsp),
            stack_frame.stack_segment,
        );
        restore_register_snapshot!(&(*CURRENT_TASK).registers);
        next_stack_frame.iretq();
    }
    */
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
    let start_ticks = timer_get_ticks();

    let freq = unsafe { 
        FREQ as u64
    };

    let ticks_duration = duration.as_micros() * freq as u128/ MICROS_PER_SECOND as u128;
    // could potentially do micros
    let end_ticks = start_ticks + ticks_duration as u64;
    println!("Ticks at {} waiting for {}", start_ticks, end_ticks);
    while timer_get_ticks() < end_ticks {
        // busy wait
        // TODO: context switch
        core::hint::spin_loop();
    }
}

pub fn init() {
    timer_set_frequency(200); // 200Hz, arbitrary
}