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

use core::time::Duration;

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
#[allow(dead_code)]
const TIMER_BIN16: u8 = 0;
#[allow(dead_code)]
const TIMER_BCD: u8 = 1;

const PIT_FREQ: u32 = 0x1234DE;

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