//! Process management syscalls
use core::mem::size_of;

use crate::{mm::translated_byte_buffer, task::{change_program_brk, current_mmap, current_munmap, current_user_token, exit_current_and_run_next, get_syscall_count, suspend_current_and_run_next}, timer::get_time_us};
use crate::mm::{PageTable, PhysAddr, PTEFlags, VirtAddr};
#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    

    let us = get_time_us();
    let sec = us / 1_000_000;
    let usec = us % 1_000_000;
    let tv = TimeVal { sec, usec };

    let buffer = translated_byte_buffer(
        current_user_token(), 
        _ts as *const u8, 
        size_of::<TimeVal>(),
    );

    let bytes = unsafe {
        core::slice::from_raw_parts(
            &tv as *const TimeVal as *const u8,
            size_of::<TimeVal>(),
        )
    };

    let mut offset = 0;

    for buf in buffer {
        let len = buf.len();
        buf.copy_from_slice(&bytes[offset..offset + len]);
        offset += len;
    }
    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    // VirtAddr::from keeps only the low 39 bits, so an illegal address like
    // isize::MAX would be silently cut into a legal-looking one.
    // User space lives below 1 << 38, so reject anything above.
    match trace_request {
        // read one byte at user address `id`
        0 => {
            if id >= 1 << 38 {
                return -1;
            }
            let pt = PageTable::from_token(current_user_token());
            let va = VirtAddr::from(id);
            let Some(pte) = pt.translate(va.floor()) else { return -1 };
            // check before trusting pte.ppn()
            if !pte.is_valid() || !pte.flags().contains(PTEFlags::U | PTEFlags::R) {
                return -1;
            }
            let pa = PhysAddr::from(pte.ppn()).0 + va.page_offset();
            unsafe { (pa as *const u8).read_volatile() as isize }
        }
        // write `data` (low 8 bits) to user address `id`
        1 => {
            if id >= 1 << 38 {
                return -1;
            }
            let pt = PageTable::from_token(current_user_token());
            let va = VirtAddr::from(id);
            let Some(pte) = pt.translate(va.floor()) else { return -1 };
            if !pte.is_valid() || !pte.flags().contains(PTEFlags::U | PTEFlags::W) {
                return -1;
            }
            let pa = PhysAddr::from(pte.ppn()).0 + va.page_offset();
            unsafe { (pa as *mut u8).write_volatile(data as u8) };
            0
        }
        // `id` is a syscall number here
        2 => get_syscall_count(id) as isize,
        _ => -1,
    }
}

/// mmap: all checks live in MemorySet::mmap
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap");
    current_mmap(start, len, prot)
}

/// munmap: all checks live in MemorySet::munmap
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    current_munmap(start, len)
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
