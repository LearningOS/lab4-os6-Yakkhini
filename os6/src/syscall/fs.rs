//! File and filesystem-related syscalls

use crate::fs;
use crate::fs::open_file;
use crate::fs::OpenFlags;
use crate::fs::Stat;
use crate::mm;
use crate::mm::translated_byte_buffer;
use crate::mm::translated_refmut;
use crate::mm::translated_str;
use crate::mm::UserBuffer;
use crate::task;
use crate::task::current_task;
use crate::task::current_user_token;
use alloc::sync::Arc;

pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        file.write(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}

pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        debug!("Invalid length.");
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        debug!(
            "Read Start: token {}, buf {}, len {}.",
            token, buf as usize, len
        );
        let answer = file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize;
        debug!("Read function result is: {}", answer);
        return answer;
    } else {
        debug!("None value.");
        return -1;
    }
}

pub fn sys_open(path: *const u8, flags: u32) -> isize {
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}

pub fn sys_close(fd: usize) -> isize {
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

// YOUR JOB: 扩展 easy-fs 和内核以实现以下三个 syscall
pub fn sys_fstat(fd: usize, st: *mut Stat) -> isize {
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();

    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }

    if let Some(file) = &inner.fd_table[fd] {
        let (id, stat_mode, nlink) = file.fstat().clone();
        drop(inner);
        let mut st_physic_address = mm::translated_refmut(current_user_token(), st);

        st_physic_address.dev = 0;
        st_physic_address.ino = id;
        st_physic_address.mode = stat_mode;
        st_physic_address.nlink = nlink;
    
        return 0;
    } else {
        return -1;
    }
}

pub fn sys_linkat(old_name_ptr: *const u8, new_name_ptr: *const u8) -> isize {
    let token = task::current_user_token();
    let old_name = translated_str(token, old_name_ptr);
    let new_name = translated_str(token, new_name_ptr);

    if old_name == new_name {
        return -1;
    }

    fs::linkat(&old_name, &new_name)
}

pub fn sys_unlinkat(_name: *const u8) -> isize {
    -1
}
