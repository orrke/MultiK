use alloc::boxed::Box;
use core::alloc::{GlobalAlloc, Layout};
use core::ptr::{null_mut, NonNull};
use x86_64::align_up;
use crate::memory::{init_hhdm, KERNEL_OFFSET};
use crate::memory::allocator::MemAlloc;
use crate::memory::page_structs::{PageEntry, PageTable, VirtualAddress};
use crate::memory::GLOBAL;
use crate::println;

struct BinaryTreeNode {
    start: *mut u8,
    size: usize,
    leaf: bool,
    has_free_space: bool,
    left_child: Option<Box<BinaryTreeNode>>,
    right_child: Option<Box<BinaryTreeNode>>,
}

impl BinaryTreeNode {
    fn new(start: *mut u8, size: usize) -> BinaryTreeNode {
        let layout = Layout::new::<BinaryTreeNode>();

        let mut ptr = unsafe { (GLOBAL.alloc(layout) as *mut BinaryTreeNode).read() };

        ptr.start = start;
        ptr.size = size;
        ptr.leaf = true;
        ptr.has_free_space = true;
        ptr.left_child = None;
        ptr.right_child = None;

        ptr
    }

    fn explore_node(&mut self, requested_size: usize) -> Option<NonNull<u8>> {
        if requested_size == self.size && self.leaf {
            self.has_free_space = false;
            return NonNull::new(self.start)
        }

        if requested_size > self.size {
            return None
        }

        if !self.leaf {
            self.explore_children(requested_size)
        } else {
            self.add_children();
            self.explore_children(requested_size)
        }
    }

    fn explore_children(&mut self, requested_size: usize) -> Option<NonNull<u8>> {
        if self.leaf {
            panic!("Node is marked as a leaf, it shouldn't have children")
        }

        let mut left_child  = self.left_child.take().expect("Node is not marked as a leaf, yet is missing children");
        let mut right_child = self.right_child.take().expect("Node is not marked as a leaf, yet is missing children");

        let mut ptr: Option<NonNull<u8>> = None;

        if left_child.has_free_space {
            ptr = left_child.explore_node(requested_size);

            if ptr.is_none() {
                ptr = right_child.explore_node(requested_size);
            }
        } else if right_child.has_free_space {
            ptr = right_child.explore_node(requested_size);
        }

        if !(left_child.has_free_space && right_child.has_free_space) {
            self.has_free_space = false;
        }

        self.left_child = Some(left_child);
        self.right_child = Some(right_child);

        ptr
    }

    fn explore_node_specific_address(&mut self, requested_size: usize, requested_address: *mut u8) {

        if self.leaf && requested_size <= self.size {
            self.has_free_space = false;
            return;
        }

        let halfway_address = unsafe { self.start.add(self.size/2) };

        let left = halfway_address > requested_address;

        if !self.leaf {
            let mut left_child  = self.left_child.take().expect("Node is not marked as a leaf, yet is missing children");
            let mut right_child = self.right_child.take().expect("Node is not marked as a leaf, yet is missing children");

            if left {
                left_child.explore_node_specific_address(requested_size, requested_address);
            } else {
                right_child.explore_node_specific_address(requested_size, requested_address);
            }

            self.left_child = Some(left_child);
            self.right_child = Some(right_child);
        } else {
            self.add_children();

            let mut left_child  = self.left_child.take().expect("Node is not marked as a leaf, yet is missing children");
            let mut right_child = self.right_child.take().expect("Node is not marked as a leaf, yet is missing children");

            if left {
                left_child.explore_node_specific_address(requested_size, requested_address);
            } else {
                right_child.explore_node_specific_address(requested_size, requested_address);
            }

            self.left_child = Some(left_child);
            self.right_child = Some(right_child);
        }
    }

    fn explore_node_free(&mut self, requested_address: *mut u8, requested_size: usize) {
        if self.leaf && requested_size == self.size && self.start == requested_address {
            self.has_free_space = true;
            return;
        }

        let halfway_address = unsafe { self.start.add(self.size/2) };

        let left = halfway_address > requested_address;

        let mut left_child = self.left_child.take().expect("Node is not marked as a leaf, yet is missing children");
        let mut right_child = self.right_child.take().expect("Node is not marked as a leaf, yet is missing children");

        if left {
            left_child.explore_node_free(requested_address, requested_size);

            if left_child.leaf && right_child.leaf {
                self.remove_children()
            }
        } else {
            right_child.explore_node_free(requested_address, requested_size);

            if left_child.leaf && right_child.leaf {
                self.remove_children()
            }
        }

        self.left_child = Some(left_child);
        self.right_child = Some(right_child);
    }

    fn add_children(&mut self) {
        if !self.leaf {
            panic!("Tried to add children to a node that is not a leaf.")
        }

        let left_child = BinaryTreeNode::new(self.start, self.size/2);
        let right_child = BinaryTreeNode::new(unsafe { self.start.add(self.size/2) }, self.size/2);

        self.leaf = false;

        self.left_child = Some(Box::new(left_child));
        self.right_child = Some(Box::new(right_child));
    }

    fn remove_children(&mut self) {
        if self.leaf {
            panic!("Tried to remove children from node that is a leaf.");
        }

        let left_child = self.left_child.take().expect("Node is not marked as a leaf, yet is missing children");
        let right_child = self.right_child.take().expect("Node is not marked as a leaf, yet is missing children");

        if left_child.leaf && right_child.leaf {
            self.left_child = None;
            self.right_child = None;

            self.leaf = true;
        } else {
            panic!("Tried to remove non leaf children from node.")
        }
    }
}

pub struct VMA {
    root_user: Option<BinaryTreeNode>,
    root_kernel: Option<BinaryTreeNode>,
}

impl VMA {
    pub const fn new() -> VMA {
        VMA {
            root_user: None,
            root_kernel: None,
        }
    }

    pub fn init(&mut self) {
        println!("Initializing VMA.");
        self.root_user = Some(BinaryTreeNode::new(0u64 as *mut u8, 1 << 47));
        self.root_kernel = Some(BinaryTreeNode::new(0xFFFF_8000_0000_0000 as *mut u8, 1 << 47));
    }

    pub unsafe fn alloc(&mut self, layout: Layout, kernel: bool) -> *mut u8 {
        if kernel {
            let size = align_up(layout.size() as u64, 4096) as usize;
            self.root_kernel.as_mut().expect("allocator is not yet initialized").explore_node(size).expect("Couldn't find a valid pointer.").as_ptr()
        } else {
            let size = align_up(layout.size() as u64, 4096) as usize;
            self.root_user.as_mut().expect("allocator is not yet initialized").explore_node(size).expect("Couldn't find a valid pointer.").as_ptr()
        }
    }

    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout, kernel: bool) {
        if kernel {
            self.root_kernel.as_mut().expect("allocator is not yet initialized").explore_node_free(ptr, layout.size());
        } else {
            self.root_user.as_mut().expect("allocator is not yet initialized").explore_node_free(ptr, layout.size());
        }
    }

    pub unsafe fn update_state(&mut self, size: usize, target_address: u64, kernel: bool) {
        if kernel {
            let corrected_size = align_up(size as u64, 4096) as usize;
            println!("Trying to allocate for size: {:#x}", corrected_size);
            self.root_kernel.as_mut().expect("allocator is not yet initialized").explore_node_specific_address(corrected_size, target_address as *mut u8);
        } else {
            let corrected_size = align_up(size as u64, 4096) as usize;
            self.root_user.as_mut().expect("allocator is not yet initialized").explore_node_specific_address(corrected_size, target_address as *mut u8);
        }
    }
}

pub struct PageTableWalker {
    start: *mut PageTable, //pointer to the PML4
}

impl PageTableWalker {
    pub const fn new() -> PageTableWalker {
        PageTableWalker {
            start: 0 as *mut PageTable,
        }
    }

    pub fn get_pml4_pointer(&self) -> *mut u8 {
        self.start as *mut u8
    }

    pub fn init(&mut self, large_alloc: &mut MemAlloc) {
        self.start = &PageTable::new_priority() as *const _ as *mut PageTable;
    }

    pub fn setup_pages(&mut self, virtual_address: *mut u8, physical_address: u64, size: usize, writable: bool, user: bool, noexec: bool) {
        println!("TRYING TO SETUP PAGES");
        println!("pml4: {:#x}", self.start as u64);

        let virtaddr_aligned = virtual_address as u64 & !(4096 - 1);
        let endaddr_aligned = (((virtual_address as usize + size) + 4096 - 1) / 4096) * 4096;

        let mut current_addr = virtaddr_aligned as usize;
        let mut current_physaddr = physical_address;

        while current_addr < endaddr_aligned {
            self.setup_page(current_addr as *mut u8, current_physaddr, writable, user, noexec);

            current_addr += 4096;
            current_physaddr += 4096;
        }
    }

    fn setup_page(&mut self, virtual_address: *mut u8, physical_address: u64, writable: bool, user: bool, noexec: bool) {
        println!("Trying to setup page for address {:#x}", virtual_address as u64);

        let virtaddr = VirtualAddress::new(virtual_address);

        let pml4_index = virtaddr.get_pml4_index();
        let pml4 = unsafe { self.start.read() };

        let mut pml4_entry = pml4.0[pml4_index];

        if !pml4_entry.is_present() {
            self.setup_page_table(&pml4_entry as *const _ as *mut PageEntry)
        }

        let pdpt_index = virtaddr.get_pdpt_index();
        let pdpt = unsafe { (pml4_entry.get_addr(true) as *mut PageTable).read() };

        let mut pdpt_entry = pdpt.0[pdpt_index];

        if !pdpt_entry.is_present() {
            self.setup_page_table(&pdpt_entry as *const _ as *mut PageEntry)
        }

        let pdt_index = virtaddr.get_pdt_index();
        let pdt = unsafe { (pdpt_entry.get_addr(true) as *mut PageTable).read() };

        let mut pdt_entry = pdt.0[pdt_index];

        if !pdt_entry.is_present() {
            self.setup_page_table(&pdt_entry as *const _ as *mut PageEntry)
        }

        let pt_index = virtaddr.get_pt_index();
        let pt = unsafe { (pdt_entry.get_addr(true) as *mut PageTable).read() };

        let mut pt_entry = pt.0[pt_index];

        if pt_entry.is_present() {
            panic!("Trying to write over an already present page!")
        }

        pt_entry.zero_entry();
        pt_entry.set_present();
        pt_entry.set_addr(physical_address);
        pt_entry.set_writable(writable);

        if user {
            pt_entry.set_user_access();
        }

        if noexec {
            pt_entry.set_noexec();
        }
    }

    fn setup_page_table(&mut self, entry_address: *mut PageEntry) {
        let next_page_table = PageTable::new();

        let hhdm = init_hhdm();
        let hhdm_offset = hhdm.offset();

        let physaddr = (&next_page_table as *const _ as u64) - hhdm_offset;

        let mut entry = unsafe { entry_address.read() };

        entry.zero_entry();

        entry.set_present();

        entry.set_addr(physaddr);
    }

    pub fn unset_page(&mut self, virtual_address: *mut u8) -> u64 {
        let virtaddr = VirtualAddress::new(virtual_address);

        let pml4_index = virtaddr.get_pml4_index();
        let pml4 = unsafe { self.start.read() };

        let mut pml4_entry = pml4.0[pml4_index];

        if !pml4_entry.is_present() {
            panic!("Trying to remove non-existent page!");
        }

        let pdpt_index = virtaddr.get_pdpt_index();
        let pdpt = unsafe { ((pml4_entry.get_addr(true) + *KERNEL_OFFSET) as *mut PageTable).read() };

        let mut pdpt_entry = pdpt.0[pdpt_index];

        if !pdpt_entry.is_present() {
            panic!("Trying to remove non-existent page!");
        }

        let pdt_index = virtaddr.get_pdt_index();
        let pdt = unsafe { ((pdpt_entry.get_addr(true) + *KERNEL_OFFSET) as *mut PageTable).read() };

        let mut pdt_entry = pdt.0[pdt_index];

        if !pdt_entry.is_present() {
            panic!("Trying to remove non-existent page!");
        }

        let pt_index = virtaddr.get_pt_index();
        let pt = unsafe { ((pdt_entry.get_addr(true) + *KERNEL_OFFSET) as *mut PageTable).read() };

        let mut pt_entry = pt.0[pt_index];

        if !pt_entry.is_present() {
            panic!("Trying to remove non-existent page!");
        }

        pt_entry.zero_entry();

        pt_entry.get_addr(false)
    }
}
