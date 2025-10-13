use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use num_traits::pow;
use x86_64::align_up;
use crate::memory::allocator::MemAlloc;
use crate::memory::{GLOBAL};
use crate::println;

const SLAB_SIZE: &[usize] = &[1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

fn list_index(layout: &Layout) -> Option<usize> {
    let required_block_size = layout.size().max(layout.align());
    SLAB_SIZE.iter().position(|&s| s >= required_block_size)
}

struct ListNode {
    next: Option<&'static mut ListNode>,
}

pub struct SlubAllocator {
    pub initialized: bool,
    start: usize,
    size: usize,
    list_heads: [Option<&'static mut ListNode>; SLAB_SIZE.len()],
    free_pointer: usize,
    free_size: usize,
}

#[allow(unsafe_op_in_unsafe_fn)]
impl SlubAllocator {
    pub const fn new() -> Self {
        const EMPTY: Option<&'static mut ListNode> = None;
        SlubAllocator {
            initialized: false,
            start: 0,
            size: 0,
            list_heads: [EMPTY; SLAB_SIZE.len()],
            free_pointer: 0,
            free_size: 0,
        }
    }

    //we take the large allocator as an argument here because otherwise it's gonna be annoying with whatever tf I'm doing with the locks in the init function
    pub fn init(&mut self, large_allocator: &mut MemAlloc) {
        //println!("Trying to init slub");

        let start_ptr = unsafe { large_allocator.alloc(Layout::from_size_align_unchecked(4096, 4096)) as usize };

        self.start = start_ptr;
        self.free_size = 4096;

        self.free_pointer = start_ptr;
        self.free_size = 4096;

        self.initialized = true;
    }

    pub unsafe fn allocate(&mut self, layout: Layout) -> *mut u8 {
        match list_index(&layout) {
            Some(index) => {
                match self.list_heads[index].take() {
                    Some(node) => {
                        self.list_heads[index] = node.next.take();
                        node as *mut ListNode as *mut u8
                    }
                    None => {
                        let size = SLAB_SIZE[index];

                        let aligned_ptr = align_up(self.free_pointer as u64, size as u64);
                        let new_free_ptr = aligned_ptr as usize + size;

                        if new_free_ptr >= self.free_pointer + self.free_size {
                            //fallback alloc to the page allocator.
                            self.free_pointer = GLOBAL.alloc(Layout::from_size_align_unchecked(4096, 4096)) as usize;
                            self.free_size = 4096;
                        }

                        self.free_pointer = new_free_ptr;
                        self.free_size -= new_free_ptr - self.free_pointer;

                        //println!("Returned pointer: {:#x}", aligned_ptr);

                        aligned_ptr as *mut u8
                    }
                }
            },
            None => panic!("Couldn't allocate block of size {}.", layout.size())
        }
    }

    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        match list_index(&layout) {
            Some(index) => {
                let new_node = ListNode {
                    next: self.list_heads[index].take(),
                };
                assert!(mem::size_of::<ListNode>() <= SLAB_SIZE[index]);
                assert!(mem::align_of::<ListNode>() <= SLAB_SIZE[index]);
                let new_node_ptr = ptr as *mut ListNode;
                unsafe {
                    new_node_ptr.write(new_node);
                    self.list_heads[index] = Some(&mut *new_node_ptr);
                }
            }
            None => panic!("Couldn't deallocate block.")
        }
    }

    pub unsafe fn allocate_uninitialized(&mut self, layout: Layout) -> *mut u8 {
        match list_index(&layout) {
            Some(index) => {
                match self.list_heads[index].take() {
                    Some(node) => {
                        self.list_heads[index] = node.next.take();
                        node as *mut ListNode as *mut u8
                    }
                    None => {
                        let size = SLAB_SIZE[index];

                        let aligned_ptr = align_up(self.free_pointer as u64, size as u64);
                        let new_free_ptr = aligned_ptr as usize + size;

                        if new_free_ptr >= self.free_pointer + self.free_size {
                            //this allocator hasn't been initialized yet, so we just get a new page by ourselves like big guys
                        }

                        self.free_pointer = new_free_ptr;
                        self.free_size -= new_free_ptr - self.free_pointer;

                        aligned_ptr as *mut u8
                    }
                }
            },
            None => panic!("Couldn't allocate block of size {}.", layout.size())
        }
    }
}
