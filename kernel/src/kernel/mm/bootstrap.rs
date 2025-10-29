use core::ptr::null_mut;

pub struct BumpAlloc {
    current_ptr: *mut u8,
    end: *mut u8,
}

impl BumpAlloc {
    pub const fn new() -> BumpAlloc {
        BumpAlloc {
            current_ptr: null_mut(),
            end: null_mut(),
        }
    }
    
    pub fn init(&mut self, start: *mut u8, end: *mut u8) {
        self.current_ptr = start;
        self.end = end;
    }

    #[allow(unsafe_op_in_unsafe_fn)]
    pub unsafe fn alloc(&mut self, size: usize) -> *mut u8 {
        if (self.current_ptr as usize + size) > self.end as usize {
            panic!("BumpAlloc overflow");
        }

        let ptr = self.current_ptr;

        self.current_ptr = self.current_ptr.add(size);

        ptr
    }

    pub unsafe fn dealloc(&mut self, _: *mut u8) {} //nothing
}
