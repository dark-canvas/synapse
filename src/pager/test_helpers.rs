use crate::pager::PhysicalAddress;
use crate::pager::MockPager;
use crate::pager::PagerError;
use crate::pager::VirtualAddress;
use crate::errors::ErrCode;
use std::vec::Vec;
use std::ops::Deref;
use std::ops::DerefMut;
use std::sync::Arc;

pub struct TestPager {
    phys_to_virt_mappings: Arc<Vec<PhysicalVirtualMapping>>,
    mock_pager: MockPager,
}

#[derive(Copy, Clone)]
pub struct PhysicalVirtualMapping {
    phys_addr: PhysicalAddress,
    virt_addr: VirtualAddress,
}

impl PhysicalVirtualMapping {
    pub fn from_vec(phys_addr: PhysicalAddress, virt_addr: Vec<u8>) -> Self {
        PhysicalVirtualMapping{
            phys_addr,
            virt_addr: VirtualAddress(virt_addr.as_ptr() as usize as crate::Address),
        }
    }

    pub fn from_array(phys_addr: PhysicalAddress, virt_addr: &[u8]) -> Self {
        PhysicalVirtualMapping{
            phys_addr,
            virt_addr: VirtualAddress(virt_addr.as_ptr() as usize as crate::Address),
        }
    }
}

/// This deref trait hides the hidden/unsafe pointer that's contained in the ConfigPage
impl Deref for TestPager {
    type Target = MockPager;

    fn deref(&self) -> &Self::Target {
        &(self.mock_pager)
    }
}

impl DerefMut for TestPager {
    fn deref_mut(&mut self) -> &mut MockPager {
        &mut self.mock_pager
    }
}

impl TestPager {
    pub fn new() -> TestPager {
        let mut mock_pager = MockPager::new();

        mock_pager.expect_get_page_size().returning(||4096);
        mock_pager.expect_get_page_mask().returning(||4095);

        TestPager {
            phys_to_virt_mappings: Arc::new(Vec::new()),
            mock_pager
        }
    }

    pub fn get_mock(&self) -> &MockPager {
        &self.mock_pager
    }

    pub fn set_mappings(&mut self, mappings: &Vec<PhysicalVirtualMapping>) {
        let arc_mappings = Arc::new(mappings.clone());
        self.phys_to_virt_mappings = Arc::clone(&arc_mappings);

        self.mock_pager.expect_get_virtual_address().returning(move |addr| {
            for mapping in arc_mappings.iter() {
                if addr.0 >= mapping.phys_addr.0 && addr.0 < mapping.phys_addr.0 + 4096 {
                    let offset = addr - mapping.phys_addr;
                    return Ok(mapping.virt_addr + offset);
                }
            }
            assert!(false, "Unexpected call to get_virtual_address");
            Err(ErrCode::Pager(PagerError::PhysicalAddressNotFound(addr)))
        });
    }
}