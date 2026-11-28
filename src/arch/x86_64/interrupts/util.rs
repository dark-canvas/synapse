pub fn cli() {
    unsafe {
        core::arch::asm!(
            "cli",
            options(nostack, preserves_flags, att_syntax)
        );
    }
}

pub fn sti() {
    unsafe {
        core::arch::asm!(
            "sti",
            options(nostack, preserves_flags, att_syntax)
        );
    }
}