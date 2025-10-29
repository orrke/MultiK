use core::alloc::{GlobalAlloc, Layout};
use core::ptr;
use core::ptr::null_mut;
use x86_64::structures::paging::Page;
use crate::memory::GLOBAL;
use crate::memory::page_alloc::FREE_PAGE_LIST;
use crate::println;

static MINIMUM_FREE_PAGE_LIST_SIZE: usize = 4;
const FREE_PAGE_LIST_SIZE: usize = 50;

#[derive(Copy, Clone, Debug)]
pub struct PageEntry(u64);

impl PageEntry {
    pub fn set_addr(&mut self, addr: u64) {
        let mask: u64 = 0x0000_FFFF_FFFF_F000;
        self.0 = (addr & mask) | self.0;
    }

    pub fn set_present(&mut self) {
        self.0 |= true as u64;
    }

    pub fn set_absent(&mut self) {
        self.0 &= !(true as u64);
    }

    pub fn set_writable(&mut self, writable: bool) {
        if writable {
            self.0 |= (writable as u64) << 1;
        } else {
            self.0 &= !(writable as u64) << 1;
        }
    }

    pub fn set_kernel_only(&mut self) {
        self.0 &= !(true as u64) << 2;
    }

    pub fn set_user_access(&mut self) {
        self.0 |= (true as u64) << 2;
    }

    pub fn set_huge_page(&mut self) {
        self.0 |= (true as u64) << 6;
    }

    pub fn unset_huge_page(&mut self) {
        self.0 &= !(true as u64) << 6;
    }

    pub fn set_noexec(&mut self) {
        self.0 |= (true as u64) << 62;
    }

    pub fn unset_noexec(&mut self) {
        self.0 &= !(true as u64) << 62;
    }

    pub fn set_standard_entry(&mut self, addr: u64) {
        self.set_addr(addr);
        self.set_present();
        self.set_user_access();
        self.set_writable(true);
        self.unset_huge_page();
        self.unset_noexec();
    }

    pub fn set_standard_kernel_entry(&mut self, addr: u64) {
        self.set_addr(addr);
        self.set_present();
        self.set_kernel_only();
        self.set_writable(true);
        self.unset_huge_page();
        self.unset_noexec();
    }

    pub fn get_addr(&mut self, kernel: bool) -> u64 {
        let mask: u64 = 0x0000_FFFF_FFFF_F000;
        let addr = self.0 & mask;

        if kernel {
            addr | 0xFFFF_8000_0000_0000
        } else {
            addr
        }
    }

    pub fn zero_entry(&mut self) {
        self.0 = 0u64;
    }

    pub fn is_present(&self) -> bool {
        let present_bit = self.0 & 1;

        if present_bit == 0 {
            false
        } else {
            true
        }
    }
}

#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct PageTable(pub [PageEntry; 512]);

impl PageTable {
    pub fn new() -> Self {
        unsafe { *FREE_PAGE_LIST.lock().allocate_page(false) }
    }

    pub fn new_priority() -> Self {
        unsafe { *FREE_PAGE_LIST.lock().allocate_page(true) }
    }
}

pub struct FreePageList {
    pages: [u64; FREE_PAGE_LIST_SIZE],
}

impl FreePageList {
    pub const fn new() -> Self {
        FreePageList {
            pages: [0; FREE_PAGE_LIST_SIZE]
        }
    }

    pub fn init(&mut self) {
        panic!("TODO")
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn allocate_page(&mut self, priority_alloc: bool) -> *mut PageTable {
        let index = self.find_first_free_page();
        let addr = self.pages[index];
        self.pages[index] = 0;
        if !priority_alloc { //a priority allocation should only be for when this list needs new pages
            self.check_and_correct_list();
        }
        addr as *mut PageTable
    }

    fn find_first_free_page(&mut self) -> usize {
        for i in 0..self.pages.len() {
            if self.pages[i] != 0 {
                return i;
            }
        }
        panic!("Could not find a free page in the list.")
    }

    fn get_free_page_len(&mut self) -> usize {
        let mut len = 0;
        for address in self.pages {
            if address != 0 {
                len += 1
            }
        }
        len
    }

    fn check_and_correct_list(&mut self) {
        if self.get_free_page_len() <= MINIMUM_FREE_PAGE_LIST_SIZE {
            let layout = Layout::new::<PageTable>();
            for i in 0..self.pages.len() {
                if self.pages[i] == 0 {
                    let ptr = unsafe {GLOBAL.alloc(layout) };
                    self.pages[i] = ptr as u64;
                }
            }
        }
        return;
    }

    pub fn add_single_page(&mut self, addr: u64) {
        for i in 0..self.pages.len() {
            if self.pages[i] == 0 {
                self.pages[i] = addr;
                return;
            }
        }
        panic!("Couldn't allocate a page, list is full!") //To remove, that's just for debugging right now
    }

    pub fn _print_list(&self) {
        for i in 0..self.pages.len() {
            //println!("{:#x}", self.pages[i]);
        }
    }
}

#[derive(PartialEq)]
pub enum BitmapState {
    FREE        = 0,
    ALLOCATED   = 1,
}

pub struct VirtualAddress(u64);

impl VirtualAddress {
    pub fn new(addr: *mut u8) -> Self {
        VirtualAddress(addr as u64)
    }

    pub fn get_pml4_index(&self) -> usize {
        ((self.0 >> 39) & 0x1FF) as usize
    }

    pub fn get_pdpt_index(&self) -> usize {
        ((self.0 >> 30) & 0x1FF) as usize
    }

    pub fn get_pdt_index(&self) -> usize {
        ((self.0 >> 21) & 0x1FF) as usize
    }

    pub fn get_pt_index(&self) -> usize {
        ((self.0 >> 12) & 0x1FF) as usize
    }
}
