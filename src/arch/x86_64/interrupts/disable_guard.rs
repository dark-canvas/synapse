pub struct DisableGuard {
}

impl DisableGuard {
    pub fn new() -> Self {
        super::cli();
        DisableGuard{}
    }
}

impl Drop for DisableGuard {
    fn drop(&mut self) {
        super::sti();
    }
}