use thiserror::Error;

use crate::pager::PagerError;

#[derive(Error, Debug)]
pub enum ErrCode {
    #[error("unknown error")]
    Unknown,

    #[error("the called function has not yet been implemented")]
    Unimplemented,

    #[error("invalid parameter")]
    InvalidParameter,

    #[error("out of memory")]
    OutOfMemory,

    #[error("error from pager module")]
    Pager(#[from] PagerError),
}

/*
 thiserror example (supposedly thiserror can be used in a nostd environment)
 use thiserror::Error;

// 1. Define a specific error enum
#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("connection failed to host: {0}")]
    ConnectionError(String),
    #[error("record not found")]
    NotFound,
}

// 2. Reference it in a top-level error enum using #[from]
#[derive(Error, Debug)]
pub enum AppError {
    // Automatically implements From<DatabaseError> for AppError
    #[error("database error occurred")]
    Database(#[from] DatabaseError),

    // You can also wrap standard library errors
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("unknown application error")]
    Unknown,
}

fn connect_db() -> Result<(), DatabaseError> {
    Err(DatabaseError::NotFound)
}

fn run_app() -> Result<(), AppError> {
    // The '?' operator uses the derived From<DatabaseError>
    connect_db()?; 
    Ok(())
}
*/