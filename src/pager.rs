use crate::errors::ErrorClass;
use crate::arch::x86_64::pager::{PhysicalAddress, VirtualAddress};

pub enum PagerErrorCode {
    Success = 0,
    PagerUnknownError = ErrorClass::Pager.get_base() + 1,
    PageNotFound,
}

// Defines a generic cross-platform pager implementation usble by certain primatives like page_based_list
// The pager is global, and so must make use of interior mutability.
// Consider:
// - Returns physical, or virtual, or both?
// - Exposes mapping routines?
pub trait Pager {
    fn get_page_size(&self) -> usize;
    
    // TODO: possibly these should return a Result<Addr, &str> instead?
    // Or should we start using somethig like?
    //    Result<Addr, ErrCode>
    //    Result<Addr, PagerErrorCode>

    // TODO: Address -> Physical/VirtualAddress
    fn allocate_physical(&self) -> Option<PhysicalAddress>;
    fn free_physical(&self, addr: PhysicalAddress);

    // WHich?
    // First one assumes the code can find a free block, which doesn't 
    // seem like it's the pager's responsibility, so probably the secnod one?
    //allocate_virtual(num: usize) -> VirtualAddress;
    fn allocate_virtual(&self, num: usize, to_addr: VirtualAddress);
    fn free_virtual(&self, num: usize, base_addr: VirtualAddress);

    // TODO: at least get physical should reutrn an Option... what about get_virtual?
    // For portability, they should both return Option
    fn get_virtual_address(&self, addr: PhysicalAddress) -> Option<VirtualAddress>;
    fn get_physical_address(&self, addr: VirtualAddress) -> Option<PhysicalAddress>;
}