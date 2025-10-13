use core::alloc::Layout;
use core::{mem, ptr};
use super::super::heap::ALLOCATOR;

const BLOCK_SIZE: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]; //no need for more, it can be handled by the buddy

fn list_index(layout: &Layout) -> Option<usize> {
    let required_block_size = layout.size().max(layout.align());
    BLOCK_SIZE.iter().position(|&s| s >= required_block_size)
}

struct ListNode {
    next: Option<&'static mut ListNode>,
}

pub struct FixedSizeBlockAllocator {
    list_heads: [Option<&'static mut ListNode>; BLOCK_SIZE.len()],
}

impl FixedSizeBlockAllocator {
    pub const fn new() -> Self {
        const EMPTY: Option<&'static mut ListNode> = None;
        FixedSizeBlockAllocator {
            list_heads: [EMPTY; BLOCK_SIZE.len()],
        }
    }

    fn fallback_alloc(&mut self, layout: Layout) -> *mut u8 {
        let mut alloc = ALLOCATOR.lock();
        unsafe { alloc.allocate(layout) }
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
                        let block_size = BLOCK_SIZE[index];
                        let block_align = block_size;
                        let layout = Layout::from_size_align(block_size, block_align).unwrap();
                        self.fallback_alloc(layout)
                    }
                }
            }
            None => panic!("Couldn't allocate block.")
        }
    }

    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        match list_index(&layout) {
            Some(index) => {
                let new_node = ListNode {
                    next: self.list_heads[index].take(),
                };
                assert!(mem::size_of::<ListNode>() <= BLOCK_SIZE[index]);
                assert!(mem::align_of::<ListNode>() <= BLOCK_SIZE[index]);
                let new_node_ptr = ptr as *mut ListNode;
                unsafe {
                    new_node_ptr.write(new_node);
                    self.list_heads[index] = Some(&mut *new_node_ptr);
                }
            }
            None => panic!("Couldn't deallocate block.")
        }
    }
}
