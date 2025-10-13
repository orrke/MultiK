use num_traits::pow;
use core::{
    ptr::self,
    alloc::Layout,
};
use core::cmp::{max, min};
use core::ptr::null_mut;
//use crate::{println, print};

static STATE_UNUSABLE: u8 = 0b00;
static STATE_FREE: u8 = 0b01;
static STATE_USED: u8 = 0b10;
static STATE_SPLIT: u8 = 0b11;

pub struct BuddyAllocator {
    start: usize,
    end: usize,
    size: usize,
    offset: u8, // the offset for the list
    free_lists: [*mut ListLink; 20], //starting from 128 bits, so, we have an offset of 4, meaning we go from 2^4 to 2^24, or about 16B - 16MiB
    bitmap: u64,
    bitmap_size: usize,
}

unsafe impl Send for BuddyAllocator {}
unsafe impl Sync for BuddyAllocator {}

impl BuddyAllocator {
    pub const fn new() -> Self {
        BuddyAllocator {
            start: 0,
            end: 0,
            size: 0,
            offset: 5, //here, equal to 4, because we need that for our bitmap calculations, it basically that, index 0 of free_lists is 4 bytes, and 1 is 5 bytes... so on
            free_lists: [ptr::null_mut(); 20],
            bitmap: 0,
            bitmap_size: 0,
        }
    }

    pub fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.bitmap = heap_start as u64; //start is expected to be a memory address, otherwise, well, idk

        let num_leaves = heap_size / (1 << self.offset);
        let total_nodes = (2 * num_leaves).saturating_sub(1);
        let bitmap_bits = total_nodes * 2;
        let bitmap_bytes = (bitmap_bits + 7) / 8;

        self.bitmap_size = bitmap_bytes;

        let ptr = self.bitmap as *mut u8;
        unsafe { ptr::write_bytes(ptr, 0, self.bitmap_size) }

        let max_block_order = self.free_lists.len() - 1 + self.offset as usize;
        let max_block_size = 1 << max_block_order;

        let aligned_heap_start = self.align_up(heap_start + self.bitmap_size, max_block_size);
        let aligned_heap_end = self.align_down(heap_start + heap_size, max_block_size);

        self.start = aligned_heap_start;
        self.end = aligned_heap_end;
        self.size = heap_size - self.bitmap_size;

        for i in 0..self.free_lists.len()+1 {
            if heap_size < ((1 << i) as usize) || i == self.free_lists.len() {
                self.insert_block(self.start, i-1);
                break;
            }
        }
    }

    fn align_up(&mut self, addr: usize, align: usize) -> usize {
        (addr + align - 1) & !(align - 1)
    }

    fn align_down(&mut self, addr: usize, align: usize) -> usize {
        addr & !(align - 1)
    }

    fn bitmap_as_slice(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(self.bitmap as *mut u8, self.bitmap_size)
        }
    }

    fn get_bitmap_value(&mut self, index: usize) -> u8 {
        let bitmap = self.bitmap_as_slice();

        let byte_index = index / 4;
        let bit_offset = (index % 4) * 2;

        (bitmap[byte_index] >> bit_offset) & 0b11
    }

    fn set_bitmap_value(&mut self, index: usize, value: u8) {
        let bitmap = self.bitmap_as_slice();

        let byte_index = index / 4;
        let bit_offset = (index % 4) * 2;

        let mask = !(0b11 << bit_offset);

        let current_byte = bitmap[byte_index];
        let cleared_byte = current_byte & mask;

        let shifted_new_state = value << bit_offset;
        let final_byte = cleared_byte | shifted_new_state;

        bitmap[byte_index] = final_byte;
    }

    fn get_node_index(&self, ptr: usize, size: usize) -> usize {
        debug_assert!(size.is_power_of_two());
        debug_assert!(ptr >= self.start && ptr < self.end);

        let order = size.trailing_zeros() as usize;

        // 1. Calculate the "level" of this node from the top of the tree.
        //    A large block has a low level number (e.g., the root is level 0).
        let max_order = self.free_lists.len() + self.offset as usize;

        let level = max_order - order;

        // 2. Calculate the number of nodes in all the levels *before* this one.
        //    There are `2^level - 1` nodes before a level starts.
        let nodes_before_level = (1 << level) - 1;

        // 3. Calculate this block's position within its own level.
        let offset_from_start = ptr - self.start;
        let position_in_level = offset_from_start / size;

        // 4. The final index is the sum.
        nodes_before_level + position_in_level
    }

    pub unsafe fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let block_size = match layout.size().checked_next_power_of_two() {
            Some(size) => max(size, 16),
            None => panic!("Unable to determine block size when allocating, most likely no more memory available."),
        };

        //assuming that, if we get here, we didn't panic just above, and so block_size is a power of 2.
        let block_index = block_size.trailing_zeros() as usize - (self.offset - 1) as usize;

        let mut ptr: *mut u8 = ptr::null_mut();

        if self.free_lists[block_index] != ptr::null_mut() {
            ptr = self.free_lists[block_index] as *mut u8;

            self.move_block_head(ptr as usize, block_index);
        } else {
            //we're gonna need to split the blocks until we get our size.

            //prepare the data we're gonna need in the context
            let mut first_free_block_index = 0;
            let mut start_ptr = 0;

            //iterate through the list until we have a free block
            for i in block_index..self.free_lists.len() {
                if self.free_lists[i] != ptr::null_mut() {
                    first_free_block_index = i;
                    start_ptr = self.free_lists[i] as usize;
                    //make sure we get out as soon as we find one
                    break;
                }
            }

            //no more memory
            if start_ptr == 0 {
                panic!("Out of memory");
            }

            //top level block, behaves differently from the rest
            if first_free_block_index == self.free_lists.len()-1 {
                let top_metadata = unsafe {
                    *(start_ptr as *const ListLink)
                };

                let next_addr = top_metadata.next;

                if next_addr != ptr::null_mut() {
                    self.free_lists[first_free_block_index] = next_addr;
                } else {
                    //we need to create the block in that case
                    let top_size = pow(2, first_free_block_index + self.offset as usize);
                    let next_addr = start_ptr + top_size;

                    if next_addr >= self.end {
                        panic!("Out of memory");
                    }

                    unsafe {
                        *(next_addr as *mut u128) = 0u128; //zero the data there, block becomes both head and tail
                    }

                    let next_idx = self.get_node_index(next_addr, top_size);
                    self.set_bitmap_value(next_idx, STATE_FREE) //free
                }
            }

            let block_to_split = start_ptr;

            for i in (block_index+1..first_free_block_index+1).rev() {
                //i = index of the current block we want to split
                let current_size = pow(2, i + self.offset as usize);

                self.split_block(block_to_split, current_size, i)
            }

            ptr = start_ptr as *mut u8;
            self.move_block(ptr as usize, block_index);
        }

        let this_idx = self.get_node_index(ptr as usize, block_size);
        self.set_bitmap_value(this_idx, STATE_USED); //mark as allocated

        ptr
    }

    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        let block_size = match layout.size().checked_next_power_of_two() {
            Some(size) => max(size, 16),
            None => panic!("Unable to determine block size when allocating, shouldn't happen because this is deallocation, not allocation."),
        };

        let block_index = block_size.trailing_zeros() as usize - (self.offset - 1) as usize;

        //this either does a cascade merge if we can merge in the first place
        //or this directly inserts the block in the list if we can't merge
        //should cover every case of the deallocation
        self.cascade_merge(ptr as usize, block_size, block_index);
    }

    fn move_block_head(&mut self, ptr: usize, index: usize) {
        let this_metadata = unsafe { *(ptr as *const ListLink) };

        let next_address = this_metadata.next;

        self.free_lists[index] = next_address;

        if next_address != null_mut() {
            unsafe { *(next_address as *mut u64) = 0u64 };
        }
    }

    //if we don't know if the block is in the head, tail or middle
    fn move_block(&mut self, ptr: usize, index: usize) {
        let this_metadata = unsafe { *(ptr as *const ListLink) };

        let prev_address = this_metadata.prev;
        let next_address = this_metadata.next;

        if prev_address == null_mut() {
            self.free_lists[index] = next_address;

            if next_address != ptr::null_mut() {
                unsafe { (*next_address).prev = ptr::null_mut(); };
            }
        } else {
            let mut prev_link = unsafe { *prev_address };

            prev_link.next = next_address;

            if next_address != ptr::null_mut() {
                unsafe { (*next_address).prev = prev_address };
            }
        }
    }

    fn split_block(&mut self, ptr: usize, size: usize, index: usize) {
        //ptr is the pointer to the start of the block to split, size is the size of the block to split
        if index <= 0 {
            panic!("cannot split further")
        }

        self.move_block(ptr, index);

        //get the size of the block right under, as well as the index of the second block we create in our split
        let sub_block_size = size / 2;
        let halfway_ptr = ptr + sub_block_size;

        //update this block in the bitmap, it's now split
        let this_idx = self.get_node_index(ptr, size);
        self.set_bitmap_value(this_idx, STATE_SPLIT);

        //get the indices of both blocks we create
        let sub_block_idx1 = self.get_node_index(ptr, sub_block_size);
        let sub_block_idx2 = self.get_node_index(halfway_ptr, sub_block_size);

        //update both sub blocks, they're now free
        self.set_bitmap_value(sub_block_idx1, STATE_FREE);
        self.set_bitmap_value(sub_block_idx2, STATE_FREE);

        let mut first_link = unsafe { &mut *(ptr as *mut ListLink) };
        let mut halfway_link = unsafe { &mut *(halfway_ptr as *mut ListLink) };

        first_link.prev = ptr::null_mut();
        first_link.next = halfway_ptr as *mut ListLink;

        halfway_link.prev = ptr as *mut ListLink;
        halfway_link.next = ptr::null_mut();

        //set the head for the block right under
        self.free_lists[index-1] = ptr as *mut ListLink;
    }

    fn insert_block(&mut self, ptr: usize, index: usize) {
        let first_ptr = self.free_lists[index];

        if first_ptr == ptr::null_mut() {
            let mut link = unsafe { &mut *(ptr as *mut ListLink) };
            link.prev = ptr::null_mut();
            link.next = ptr::null_mut();

            self.free_lists[index] = ptr as *mut ListLink;
        } else {
            let mut link = unsafe { &mut *(ptr as *mut ListLink) };
            link.prev = ptr::null_mut();
            link.next = first_ptr;

            self.free_lists[index] = ptr as *mut ListLink;
        }
    }

    fn merge_block(&mut self, ptr1: usize, ptr2: usize, size: usize, index: usize) {
        //Okay, so, a merge should only be called when the present block has just been freed, and the buddy was free
        //This means that, the current block shouldn't be a part of the linked list.

        //remove the buddy block from the linked list
        self.move_block(ptr2, index);

        //get both indices of the blocks in the linked list
        let idx1 = self.get_node_index(ptr1, size);
        let idx2 = self.get_node_index(ptr2, size);

        //set both states to unusable
        self.set_bitmap_value(idx1, STATE_UNUSABLE);
        self.set_bitmap_value(idx2, STATE_UNUSABLE);

        //get the larger block's index
        let idx_parent = self.get_node_index(ptr1, size*2);
        self.set_bitmap_value(idx_parent, STATE_FREE); //set the parent as free

        //get the index of the parent in the free_lists array
        let parent_index = index + 1;

        //get the pointer to the first block in the linked list
        let first_block_pointer = self.free_lists[parent_index];

        if first_block_pointer == ptr::null_mut() {
            self.free_lists[parent_index] = ptr::null_mut();
            unsafe { *(ptr1 as *mut u128) = 0u128 }
        } else {
            let this_metadata = unsafe { &mut *first_block_pointer };
            this_metadata.next = first_block_pointer;
            this_metadata.prev = ptr::null_mut();

            //replace the previous address in the first block (which should previously be 0) by this block's address
            unsafe { *(first_block_pointer as *mut u64) = ptr1 as u64 };

            unsafe { (&mut *first_block_pointer).prev = ptr1 as *mut ListLink }

        }
    }

    fn cascade_merge(&mut self, ptr1: usize, size: usize, index: usize) {
        let mut possible_merge = true;
        let mut current_size = size;
        let mut current_index = index;
        let mut current_ptr = ptr1;

        while possible_merge {
            if current_index != self.free_lists.len()-1 {
                let buddy_address = ptr1 ^ current_size;
                let buddy_idx = self.get_node_index(buddy_address, current_size);
                let buddy_state = self.get_bitmap_value(buddy_idx);

                if buddy_state == STATE_FREE {
                    self.merge_block(ptr1, buddy_address, current_size, current_index);

                    possible_merge = true;
                } else {
                    possible_merge = false;
                }

                current_ptr = ptr1.min(buddy_address);
                current_index += 1;
                current_size *= 2;
            } else {
                possible_merge = false;
                //since this is the highest size, we can't merge further, so we just add it to the linked list.
            }
        }

        self.insert_block(current_ptr, current_index);
    }
}

#[repr(packed)]
#[derive(Copy, Clone)]
struct ListLink {
    prev: *mut ListLink,
    next: *mut ListLink,
}

impl ListLink {
    fn new() -> Self {
        ListLink {
            prev: ptr::null_mut(),
            next: ptr::null_mut(),
        }
    }
}

/*
Bits	State	Meaning
11	Split	This block is not usable; it has been split into its two children.
10	Allocated	This block is whole and has been given to the user.
01	Free	This block is whole, free, and should be in the corresponding free list.
00	Unavailable	This block doesn't exist; it's part of a larger free block (its parent is marked Free).
 */


