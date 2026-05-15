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

use x86_64::structures::idt::InterruptStackFrame;
use x86_64::registers::model_specific::Msr;

pub fn init() {
    // TODO
}

// It's debateable whether this belongs here, or in the IDT module...  ultimately it'll defer to common multitasking code...
pub extern "x86-interrupt" fn timer_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    // NOP

    // TODO: need a better/common way to signal EOI (for all interrupts) and in the x2apic mod
    unsafe { 
        let mut apic_eoi_msr = x86_64::registers::model_specific::Msr::new(0x80b) };
        apic_eoi_msr.write(0x0)
    };
}