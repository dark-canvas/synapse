pub fn enable() {
    unsafe {
        core::arch::asm!(
            "cli",
            options(nostack, preserves_flags, att_syntax)
        );
    }
}

pub fn disable() {
    unsafe {
        core::arch::asm!(
            "sti",
            options(nostack, preserves_flags, att_syntax)
        );
    }
}