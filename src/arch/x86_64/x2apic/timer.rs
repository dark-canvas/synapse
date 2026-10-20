use core::time::Duration;

use x2apic::lapic::{TimerDivide, TimerMode};

use crate::arch::x86_64::{pit::timer_delay, smp::{CpuState, per_cpu_data::get_cpu_state_base}};

// TODO: run multiple calibrate loops and average the results?
pub fn timer_init() {
    let cpu_state = CpuState::get_local_cpu_state();
    let calibration_ms = 50;
    let calibration_duration = Duration::from_millis(calibration_ms);

    unsafe {
        cpu_state.lapic.disable_timer();
        cpu_state.lapic.set_timer_divide(TimerDivide::Div1);
        cpu_state.lapic.set_timer_mode(TimerMode::OneShot);
        cpu_state.lapic.set_timer_initial(0xffffffff);

        cpu_state.lapic.enable_timer();
        let initial_ticks = cpu_state.lapic.timer_current();
        timer_delay(calibration_duration);
        let final_ticks = cpu_state.lapic.timer_current();

        // ticks count down, so the delta is start - end, not end - start...
        let ticks_per_ms = (initial_ticks - final_ticks) / calibration_ms as u32;
        println!("lapic rate {} ticks/ms", ticks_per_ms);

        cpu_state.lapic_timer_ticks_per_ms = ticks_per_ms;
    }

    // 100 times per second... always single shot, for now.
    timer_set(Duration::from_millis(10));
}

pub fn timer_set(dur: Duration) {
    let cpu_state = CpuState::get_local_cpu_state();
    // TODO: determine time range here... or maybe better, ensure values are asserted with into()
    let ticks = dur.as_micros() as u64 * cpu_state.lapic_timer_ticks_per_ms as u64 / 1000;
 
    unsafe {
        cpu_state.lapic.set_timer_divide(TimerDivide::Div1);
        cpu_state.lapic.set_timer_mode(TimerMode::OneShot);
        cpu_state.lapic.set_timer_initial(ticks as u32);
        cpu_state.lapic.enable_timer();
    }
}