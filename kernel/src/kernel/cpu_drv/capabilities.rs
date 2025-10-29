use core::ptr::null_mut;

pub fn setup_capability_db() {}

struct CSpace {
    cnode_root: L1CapabilityNode,
}

struct L1CapabilityNode {
    children: [*mut L2CapabilityNode; 0b1<<24], //2^24
}

impl L1CapabilityNode {
    pub const fn new() -> Self {
        L1CapabilityNode { children: [null_mut(); 0b1<<24] }
    }
}

struct L2CapabilityNode {
    children: [u64; 256],
}

impl L2CapabilityNode {
    pub const fn new() -> Self {
        L2CapabilityNode { children: [0; 256] }
    }
}
