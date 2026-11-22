use core::arch::asm;
use x86_64::{registers::control::Cr3};

pub fn get_cr3() -> u64 {
    let (pdbr, flags) = Cr3::read_raw();

    pdbr.start_address().as_u64() | (u64::from(flags))
}

pub fn set_cr3(value: u64) {
    unsafe {
        asm!(
            "mov {value}, %rax",
            "mov %rax, %cr3",
            value = in(reg) value,
            out("rax") _,
            options(att_syntax)
        );
    }
}