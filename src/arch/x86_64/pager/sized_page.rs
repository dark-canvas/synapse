use crate::types::Address;

#[allow(dead_code)]
#[derive(PartialEq, Copy, Clone)]
pub enum SizedPage {
    Page4KB(Address),
    Page2MB(Address),
    Page1GB(Address),
}
