use crate::arch::x86_64::pager;
use acpi::{Handler, PhysicalMapping, PciAddress};
use core::ptr::NonNull;


#[derive(Clone)]
pub struct SynapseAcpiHandler;

impl Handler for SynapseAcpiHandler {
    
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        // return the physical address using the physical mirror
        let virtual_address = physical_address + pager::PHYSICAL_OFFSET as usize;
        let virtual_address = NonNull::new(virtual_address as *mut T).unwrap();
        
        PhysicalMapping{
            physical_start: physical_address,
            virtual_start: virtual_address,
            region_length: size,
            mapped_length: size,
            handler: self.clone(),
        }
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {
        // No operation needed: the physical mirror is persistent
    }

    // Hardware I/O Methods (Safe to stub out if only parsing tables) ----
    fn read_u8(&self, address: usize) -> u8 { unimplemented!() }
    fn read_u16(&self, address: usize) -> u16 { unimplemented!() }
    fn read_u32(&self, address: usize) -> u32 { unimplemented!() }
    fn read_u64(&self, address: usize) -> u64 { unimplemented!() }
    fn write_u8(&self, address: usize, value: u8) { unimplemented!() }
    fn write_u16(&self, address: usize, value: u16) { unimplemented!() }
    fn write_u32(&self, address: usize, value: u32) { unimplemented!() }
    fn write_u64(&self, address: usize, value: u64) { unimplemented!() }
    fn read_io_u8(&self, port: u16) -> u8 { unimplemented!() }
    fn read_io_u16(&self, port: u16) -> u16 { unimplemented!() }
    fn read_io_u32(&self, port: u16) -> u32 { unimplemented!() }
    fn write_io_u8(&self, port: u16, value: u8) { unimplemented!() }
    fn write_io_u16(&self, port: u16, value: u16) { unimplemented!() }
    fn write_io_u32(&self, port: u16, value: u32) { unimplemented!() }
    fn read_pci_u8(&self, address: PciAddress, offset: u16) -> u8 { unimplemented!() }
    fn read_pci_u16(&self, address: PciAddress, offset: u16) -> u16 { unimplemented!() }
    fn read_pci_u32(&self, address: PciAddress, offset: u16) -> u32 { unimplemented!() }
    fn write_pci_u8(&self, address: PciAddress, offset: u16, value: u8) { unimplemented!() }
    fn write_pci_u16(&self, address: PciAddress, offset: u16, value: u16) { unimplemented!() }
    fn write_pci_u32(&self, address: PciAddress, offset: u16, value: u32) { unimplemented!() }
    fn nanos_since_boot(&self) -> u64 { 0 } // obvoiusly not accurate
    fn stall(&self, microseconds: u64) {
        self.sleep(microseconds)
    }
    fn sleep(&self, microseconds: u64) {
        // I don't have a good way to do this currently... the duration isn't accurate (obv.)
        for _ in 0..(microseconds * 1000) { core::hint::spin_loop(); }
    }
}
