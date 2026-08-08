use x86_64::registers::control::Cr3;


pub fn get_cr3() -> u64 {
    let (pdbr, flags) = Cr3::read_raw();

    pdbr.start_address().as_u64() | (u64::from(flags))
}