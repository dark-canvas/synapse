use num_traits::PrimInt;

pub trait RelocationType: PrimInt {}

impl RelocationType for u32 {}
impl RelocationType for u64 {}
impl RelocationType for u128 {}

pub struct Relocation<T> {
    addr: *const u8,
    mask: T,
}

impl<MaskT: RelocationType> Relocation<MaskT> {
    pub fn new(addr: &u8, mask: MaskT) -> Relocation<MaskT> {
        assert!( mask.trailing_zeros() & 0x7 == 0, "mask target must be byte aligned");
        Relocation {
            addr: addr as *const u8,
            mask: mask
        }
    }

    pub fn set<ValueT: PrimInt + core::fmt::LowerHex>(&self, value: ValueT) {
        assert_eq!(
            core::mem::size_of_val(&value) * 8, 
            self.mask.count_ones().try_into().unwrap(),
            "value must be same size as mask target"
        );
        let byte_offset = (self.mask.trailing_zeros() / 8) as usize;
        println!("Offseting {:#x} by {} bytes", self.addr as usize, byte_offset);

        unsafe {
            let patch_ptr = self.addr.add( byte_offset ) as *mut ValueT;

            println!("Patching {} byte value at {:#x} with {:#x}", 
                core::mem::size_of_val(&value), 
                patch_ptr as usize,
                value);

            patch_ptr.write_unaligned(value);
        }
    }

    pub fn test_and_set<ValueT: PrimInt + core::fmt::LowerHex>(&self, value: ValueT) {
        let byte_offset = (self.mask.trailing_zeros() / 8) as usize;
        let num_bytes = core::mem::size_of_val(&value);

        let mut expected = (0x11 * num_bytes) as u8;
        for i in 0..num_bytes {
            unsafe {
                let patch_ptr = self.addr.add( byte_offset + i );
                assert_eq!(*patch_ptr, expected);
            }
            expected -= 0x11;
        }

        self.set(value);
    }
}
