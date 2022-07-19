//! Implementation of [`Processor`] and Intersection of control flow
//!
//! Here, the continuous operation of user apps in CPU is maintained,
//! the current running state of CPU is recorded,
//! and the replacement and transfer of control flow of different applications are executed.

use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use crate::{config, mm};
use alloc::sync::Arc;
use lazy_static::*;

/// Processor management structure
pub struct Processor {
    /// The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,
    /// The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(|task| Arc::clone(task))
    }
}

lazy_static! {
    /// PROCESSOR instance through lazy_static!
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

/// The main part of process execution and scheduling
///
/// Loop fetch_task to get the process that needs to run,
/// and switch the process through __switch
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get token of the address space of current task
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    let token = task.inner_exclusive_access().get_user_token();
    token
}

/// Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}

/// mmap function
pub fn mmap(start: usize, len: usize, port: usize) -> isize {

    debug!("Get into mmap function.");

    if (start % config::PAGE_SIZE != 0) || (port & !0x7 != 0) || (port & 0x7 == 0) {
        return -1;
    }

    let start_address = mm::VirtAddr(start);
    let end_address = mm::VirtAddr(start + len);

    let map_permission =
        mm::MapPermission::from_bits((port as u8) << 1).unwrap() | mm::MapPermission::U;

    for vpn in mm::VPNRange::new(mm::VirtPageNum::from(start_address), end_address.ceil()) {
        println!("[debug] The vpn is in range. current: {}", usize::from(vpn));
        debug!("The vpn is in range. current: {}", usize::from(vpn));
        if let Some(pte) = take_current_task()
            .unwrap()
            .inner_exclusive_access()
            .memory_set
            .translate(vpn)
        {
            if pte.is_valid() {
                return -1;
            }
        };
    }

    take_current_task()
        .unwrap()
        .inner_exclusive_access()
        .memory_set
        .insert_framed_area(start_address, end_address, map_permission);

    for vpn in mm::VPNRange::new(mm::VirtPageNum::from(start_address), end_address.ceil()) {
        match take_current_task()
            .unwrap()
            .inner_exclusive_access()
            .memory_set
            .translate(vpn)
        {
            Some(pte) => {
                if pte.is_valid() == false {
                    return -1;
                }
            }
            None => {
                return -1;
            }
        }
    }

    return 0;
}

/// munmap function
pub fn munmap(start: usize, len: usize) -> isize {
        if start % config::PAGE_SIZE != 0 {
        return -1;
    }

    let start_address = mm::VirtAddr(start);
    let end_address = mm::VirtAddr(start + len);

    for vpn in mm::VPNRange::new(mm::VirtPageNum::from(start_address), end_address.ceil()) {
        match current_task()
            .unwrap()
            .inner_exclusive_access()
            .memory_set
            .translate(vpn)
        {
            Some(pte) => {
                if pte.is_valid() {
                    return -1;
                }
            }
            None => {
                return -1;
            }
        }
    }

    for vpn in mm::VPNRange::new(mm::VirtPageNum::from(start_address), end_address.ceil()) {
        current_task()
            .unwrap()
            .inner_exclusive_access()
            .memory_set
            .remove_area_with_start_vpn(vpn);
    }

    for vpn in mm::VPNRange::new(mm::VirtPageNum::from(start_address), end_address.ceil()) {
        if let Some(pte) = current_task()
            .unwrap()
            .inner_exclusive_access()
            .memory_set
            .translate(vpn)
        {
            if pte.is_valid() {
                return -1;
            }
        };
    }

    return 0;
}
