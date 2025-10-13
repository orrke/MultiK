use core::alloc::Layout;
use core::cmp::{min, PartialEq};
use core::ptr::{null_mut, NonNull};
use num_traits::pow;
use spin::Mutex;
use crate::memory::page_structs::{BitmapState, FreePageList};

pub static FREE_PAGE_LIST: Mutex<FreePageList> = Mutex::new(FreePageList::new());

static BUDDY_BLOCK_SIZES: &[usize] = &[4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608, 16777216];

fn get_order(layout: &Layout) -> Option<usize> {
    let required_block_size = layout.size().max(layout.align());
    BUDDY_BLOCK_SIZES.iter().position(|&s| s >= required_block_size)
}

pub struct Bitmap {
    bitmap: *mut u64,
    size: usize,
}

impl Bitmap {
    pub const fn new() -> Self {
        Bitmap {
            bitmap: null_mut(),
            size: 0,
        }
    }

    pub fn init(&mut self, start_addr: u64, size: usize) {
        self.bitmap = start_addr as *mut u64;
        self.size = size;
    }

    pub fn get_state(&mut self, address: u64) -> BitmapState {
        let index = address / 4096;

        let bit_index = index / 8;
        let bit_offset = index % 8;

        let ptr = unsafe { self.bitmap.add(bit_index as usize) };

        let state = (unsafe { ptr.read() }  >> bit_offset) & 1;

        if state == 1 {
            BitmapState::ALLOCATED
        } else {
            BitmapState::FREE
        }
    }

    fn toggle_bitmap_state(&mut self, address: u64) {
        let index = address / 4096;

        let bit_index = index / 8;
        let bit_offset = index % 8;

        let mask = 1u8 << bit_offset;

        let ptr = unsafe { self.bitmap.add(bit_index as usize) } as *mut u8;

        unsafe { *ptr ^= mask };
    }

    pub unsafe fn update_bitmap(&mut self, address: u64, order: usize, state: BitmapState) {
        for i in 0..pow(2, order) {
            if self.get_state(address + (i*4096) as u64) != state {
                self.toggle_bitmap_state(address + (i*4096) as u64);
            }
        }
    }

    unsafe fn get_bitmap_state(&mut self, address: u64, order: usize) -> BitmapState {
        for i in 0..pow(2, order) {
            if self.get_state(address + (i*4096) as u64) == BitmapState::ALLOCATED {
                return BitmapState::ALLOCATED;
            }
        }

        BitmapState::FREE
    }
}

#[derive(Debug, Copy, Clone)]
struct BuddyLinkedListNode {
    //THESE ARE PHYSICAL ADDRESSES!
    prev: Option<NonNull<BuddyLinkedListNode>>,
    next: Option<NonNull<BuddyLinkedListNode>>,
}

impl BuddyLinkedListNode {
    unsafe fn set_next(this: Option<NonNull<BuddyLinkedListNode>>, next: Option<NonNull<BuddyLinkedListNode>>, offset: u64) {
        this.expect("Passed a null pointer.").add(offset as usize).read().next = next;
    }
}

//lives in a higher half direct map
pub struct BuddyAlloc {
    start: u64,
    size: usize,
    hhdm_offset: u64,
    bitmap: Bitmap,
    free_lists: [Option<NonNull<BuddyLinkedListNode>>; BUDDY_BLOCK_SIZES.len()],
}

unsafe impl Sync for BuddyAlloc {}
unsafe impl Send for BuddyAlloc {}

impl BuddyAlloc {
    pub const fn new() -> Self {
        BuddyAlloc {
            start: 0,
            size: 0,
            hhdm_offset: 0,
            bitmap: Bitmap::new(),
            free_lists: [None; BUDDY_BLOCK_SIZES.len()],
        }
    }

    pub fn init(&mut self, start_addr: u64, size: usize, bitmap_start: u64, hhdm_offset: u64) {
        self.start = start_addr;
        self.size = size;
        self.hhdm_offset = hhdm_offset;

        self.bitmap.init(bitmap_start, size);
    }

    pub unsafe fn alloc(&mut self, size: usize) -> u64 {
        let corrected_size = size.checked_next_power_of_two().expect("Couldn't get corrected size.");
        let order = corrected_size.trailing_zeros() as usize;

        match self.free_lists[order] {
            Some(free_block) => {
                let phys_addr = free_block.as_ptr() as u64;

                self.free_lists[order] = free_block.add(self.hhdm_offset as usize).read().next;

                self.bitmap.update_bitmap(phys_addr, order, BitmapState::ALLOCATED);

                phys_addr
            },
            None => {
                let free_order = self.find_first_free_order(order);
                let phys_addr = self.free_lists[free_order].expect("find_first_free_order function failed.").as_ptr() as u64;

                for order in (order+1..free_order+1).rev() {
                    self.split_block(order)
                }

                let next_block = self.free_lists[order].expect("Split did not work.").read().next;
                self.free_lists[order] = next_block;

                self.bitmap.update_bitmap(phys_addr, order, BitmapState::ALLOCATED);

                phys_addr
            }
        }
    }

    pub unsafe fn free(&mut self, address: u64, size: usize) {
        let corrected_size = size.checked_next_power_of_two().expect("Couldn't get size.");
        let order = corrected_size.trailing_zeros() as usize;

        let mut merge_possible = true;
        let mut current_order = order;
        let mut current_address = address;

        self.bitmap.update_bitmap(current_address, current_order, BitmapState::FREE);

        while merge_possible {
            merge_possible = self.merge_block_with_buddy(address, current_order);
            current_order += 1;

            let current_size = pow(2, current_order);
            let buddy_address = current_address ^ current_size;

            current_address = current_address.min(buddy_address);

            if current_order >= BUDDY_BLOCK_SIZES.len() {
                merge_possible = false;
            }

            self.bitmap.update_bitmap(current_address, current_order, BitmapState::FREE);
        }
    }

    //splits the first block for that order in the linked list
    unsafe fn split_block(&mut self, order: usize) {
        if order == 0 {
            panic!("Tried to split a block of the smallest order.")
        }

        let address = self.free_lists[order].expect("No free block for that order.");

        if self.bitmap.get_bitmap_state(address.as_ptr() as u64, order) == BitmapState::FREE {
            let linked_list_node = *((address.as_ptr() as u64 + self.hhdm_offset) as *mut BuddyLinkedListNode);

            self.free_lists[order] = linked_list_node.next;

            let first_block = self.free_lists[order];
            let second_block = Some(self.free_lists[order].unwrap().add(pow(2, order)/2));

            BuddyLinkedListNode::set_next(first_block, second_block, self.hhdm_offset);
            BuddyLinkedListNode::set_next(second_block, self.free_lists[order-1], self.hhdm_offset);

            self.free_lists[order-1] = first_block;
        } else {
            panic!("Tried to split non fully free block.");
        }
    }

    unsafe fn find_first_free_order(&mut self, order: usize) -> usize {
        for i in order+1..BUDDY_BLOCK_SIZES.len() {
            if self.free_lists[order].is_some() {
                return i;
            }
        }

        panic!("Couldn't find a free order in the list.")
    }

    unsafe fn merge_block_with_buddy(&mut self, address: u64, order: usize) -> bool {
        let size = pow(2, order) as usize;

        //get buddy address
        let buddy_address = address ^ size as u64;

        match self.bitmap.get_bitmap_state(buddy_address, order) {
            BitmapState::FREE => {
                //get buddy node in teh linked list
                let mut buddy_node = *((buddy_address + self.hhdm_offset) as *mut BuddyLinkedListNode);

                //remove the buddy from the linked list
                if buddy_node.prev.is_some() {
                    buddy_node.prev.unwrap().add(self.hhdm_offset as usize).read().next = buddy_node.next;
                } else {
                    self.free_lists[order] = buddy_node.next;
                }

                if buddy_node.next.is_some() {
                    buddy_node.next.unwrap().add(self.hhdm_offset as usize).read().prev = buddy_node.prev;
                }

                buddy_node.prev = None;
                buddy_node.next = self.free_lists[order+1];
                self.free_lists[order+1] = NonNull::new(&buddy_node as *const _ as *mut BuddyLinkedListNode);

                true
            }
            _ => false
        }
    }

    pub fn mark_as_allocated(&mut self, address: u64, size: usize) {
        unsafe { self.bitmap.update_bitmap(address, size, BitmapState::ALLOCATED) };
    }
}
