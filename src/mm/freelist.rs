static mut FREE_LIST_HEAD: usize = 0;

pub fn init_frame_allocator(start: usize, end: usize) {
    assert!(start % 4096 == 0);
    assert!(end % 4096 == 0);
    assert!(start < end);

    let mut addr = start;
    unsafe {
        FREE_LIST_HEAD = addr as usize;
    }

    while addr < end {
        let next_addr = addr + 4096;
        let ptr = addr as *mut usize;
        if next_addr >= end {
            unsafe {
                ptr.write(0);  // null
            }
        } else {
            unsafe {
                ptr.write(next_addr as usize);
            }
        }
        addr = next_addr;
    }
}

pub fn alloc_frame() -> usize {
    let ptr = unsafe { FREE_LIST_HEAD as *mut usize };
    if ptr.is_null() {
        panic!("Out of memory");
    }
    let next_addr = unsafe { *ptr };
    unsafe {
        FREE_LIST_HEAD = next_addr;
        (ptr as *mut u8).write_bytes(0, 4096);
    }
    ptr as usize
}

pub fn free_frame(addr: usize) {
    assert!(addr % 4096 == 0);
    let ptr = addr as *mut usize;
    unsafe {
        *ptr = FREE_LIST_HEAD;
        FREE_LIST_HEAD = addr;
    }
}
