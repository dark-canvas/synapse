use thiserror::Error;

#[allow(dead_code)]
#[derive(Error, Debug, PartialEq)]
pub enum SmpError {
    #[error("SMP is not initialized")]
    NotInitialized,
}