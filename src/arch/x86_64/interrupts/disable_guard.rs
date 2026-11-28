pub struct DisableGuard {
}

impl DisableGuard {
    pub fn new() -> Self {
        super::enable();
        DisableGuard{}
    }
}

impl Drop for DisableGuard {
    fn drop(&mut self) {
        super::disable();
    }
}