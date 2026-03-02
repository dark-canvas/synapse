pub struct Stack<'a> {
    size: usize,
    base: &'a mut[usize],
    pointer: usize,
}

impl<'a> Stack<'a> {
    pub fn new(base: usize, size: usize) -> Stack<'static> {
        Stack { base: unsafe { core::slice::from_raw_parts_mut(base as *mut usize, size) }, size, pointer: 0 }
    }

    pub fn top(&self) -> usize {
        self.base.as_ptr() as usize + (self.size * core::mem::size_of::<usize>())
    }

    pub fn push(&mut self, value: usize) {
        if self.pointer >= self.size {
            panic!("Stack overflow");
        }
        self.base[self.pointer] = value;
        self.pointer += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_stack() {
        let stack = Stack::new(0x1000, 10);

        assert_eq!(stack.size, 10);
        assert_eq!(stack.base.as_ptr() as usize, 0x1000);
        assert_eq!(stack.top(), 0x1000 + 10 * core::mem::size_of::<usize>());
    }

    #[test]
    fn test_push_stack() {
        let stack_data = [0; 10];
        let mut stack = Stack::new(stack_data.as_ptr() as usize, 10);
        stack.push(0x12345678);
        assert_eq!(stack.base[0], 0x12345678);
    }
}
