use crate::arch::x86_64::util::register_snapshot::RegisterSnapshot;
use crate::Address;
use crate::errors::ErrCode;

const TASK_MAP_BASE_ADDRESS: Address = 0xFFFFFFF010000000;
const TASK_MAP_ITEM_SIZE: usize = 65536;

pub type TaskHandle = u32;

pub struct Task {
    kernel_stack: [u8; 16*1024],
    io_bitmap: [u8; 8193],
    cr3: Address,
    gs_base: Address,
    registers: RegisterSnapshot,
    //fp_registers: FPRegisterSnapshot,
    //avx_snapshot: AVXRegisterSnapshot,
}

pub struct TaskMap {
}

impl TaskMap {

    pub fn get_task(handle: TaskHandle) -> Result<&'static Task, ErrCode> {
        Err(ErrCode::Unimplemented)
    }

    pub fn new() -> Result<&'static Task, ErrCode> {
        Err(ErrCode::Unimplemented)
    }

    pub fn free(handle: TaskHandle) -> Result<(), ErrCode> {
        Err(ErrCode::Unimplemented)
    }
}

