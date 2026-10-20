pub trait TaskHandle {
    fn is_null(&self) -> bool;
    fn set_null(&mut self);
}