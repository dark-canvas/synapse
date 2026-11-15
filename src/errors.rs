use thiserror::Error;

use crate::{pager::PagerError, smp::SmpError};

#[allow(dead_code)]
#[derive(Error, Debug, PartialEq)]
pub enum ErrCode {
    #[error("unknown error")]
    Unknown,

    #[error("the called function has not yet been implemented")]
    Unimplemented,

    #[error("invalid parameter")]
    InvalidParameter,

    #[error("invalid handle")]
    InvalidHandle,

    #[error("out of bounds")]
    OutOfBounds,

    #[error("out of memory")]
    OutOfMemory,

    #[error("out of handles")]
    OutOfHandles,

    #[error("error from pager module")]
    Pager(#[from] PagerError),

    #[error("error from smp module")]
    Smp(#[from] SmpError),
}
