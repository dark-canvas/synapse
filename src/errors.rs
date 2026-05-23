const ERROR_CLASS_SIZE: isize = 1000;

pub enum ErrorClass {
    Generic = 1,
    Pager,
}

impl ErrorClass {
    pub const fn get_base(self) -> isize {
        self as isize * ERROR_CLASS_SIZE
    }
}

pub enum GenericErrCode {
    Success = 0,
    GenericErrorUnknown = ErrorClass::Generic as isize * ERROR_CLASS_SIZE + 1,
}