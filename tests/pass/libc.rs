// ignore-windows: No libc on Windows
// compile-flags: -Zmiri-disable-isolation

#![feature(io_error_more)]
#![feature(rustc_private)]

extern crate libc;

fn tmp() -> std::path::PathBuf {
    std::env::var("MIRI_TEMP")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir())
}

/// Test allocating variant of `realpath`.
fn test_posix_realpath_alloc() {
    use std::ffi::OsString;
    use std::ffi::{CStr, CString};
    use std::fs::{remove_file, File};
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    let buf;
    let path = tmp().join("miri_test_libc_posix_realpath_alloc");
    let c_path = CString::new(path.as_os_str().as_bytes()).expect("CString::new failed");

    // Cleanup before test.
    remove_file(&path).ok();
    // Create file.
    drop(File::create(&path).unwrap());
    unsafe {
        let r = libc::realpath(c_path.as_ptr(), std::ptr::null_mut());
        assert!(!r.is_null());
        buf = CStr::from_ptr(r).to_bytes().to_vec();
        libc::free(r as *mut _);
    }
    let canonical = PathBuf::from(OsString::from_vec(buf));
    // Cleanup after test.
    remove_file(&path).unwrap();

    assert_eq!(path.file_name(), canonical.file_name());
}

/// Test non-allocating variant of `realpath`.
fn test_posix_realpath_noalloc() {
    use std::ffi::{CStr, CString};
    use std::fs::{remove_file, File};
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;

    let path = tmp().join("miri_test_libc_posix_realpath_noalloc");
    let c_path = CString::new(path.as_os_str().as_bytes()).expect("CString::new failed");

    let mut v = vec![0; libc::PATH_MAX as usize];

    // Cleanup before test.
    remove_file(&path).ok();
    // Create file.
    drop(File::create(&path).unwrap());
    unsafe {
        let r = libc::realpath(c_path.as_ptr(), v.as_mut_ptr());
        assert!(!r.is_null());
    }
    let c = unsafe { CStr::from_ptr(v.as_ptr()) };
    let canonical = PathBuf::from(c.to_str().expect("CStr to str"));
    // Cleanup after test.
    remove_file(&path).unwrap();

    assert_eq!(path.file_name(), canonical.file_name());
}

/// Test failure cases for `realpath`.
fn test_posix_realpath_errors() {
    use std::convert::TryInto;
    use std::ffi::CString;
    use std::fs::{create_dir_all, remove_dir, remove_file};
    use std::io::ErrorKind;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;

    // Test non-existent path returns an error.
    let c_path = CString::new("./nothing_to_see_here").expect("CString::new failed");
    let r = unsafe { libc::realpath(c_path.as_ptr(), std::ptr::null_mut()) };
    assert!(r.is_null());
    let e = std::io::Error::last_os_error();
    assert_eq!(e.raw_os_error(), Some(libc::ENOENT));
    assert_eq!(e.kind(), ErrorKind::NotFound);

    // Test that a long path returns an error.
    //
    // Linux first checks if the path to exists and macos does not.
    // Using an existing path ensures all platforms return `ENAMETOOLONG` given a long path.
    //
    // Rather than creating a bunch of directories, we create two directories containing symlinks.
    // Sadly we can't avoid creating directories and instead use a path like "./././././" or "./../../" as linux
    // appears to collapse "." and ".." before checking path length.
    let path = tmp();

    // The directories we will put symlinks in.
    let x = path.join("x/");
    let y = path.join("y/");

    // The symlinks in each directory pointing to each other.
    let yx_sym = y.join("x");
    let xy_sym = x.join("y");

    // Cleanup before test.
    remove_file(&yx_sym).ok();
    remove_file(&xy_sym).ok();
    remove_dir(&x).ok();
    remove_dir(&y).ok();

    // Create directories.
    create_dir_all(&x).expect("dir x");
    create_dir_all(&y).expect("dir y");

    // Create symlinks between directories.
    symlink(&x, &yx_sym).expect("symlink x");
    symlink(&y, &xy_sym).expect("symlink y ");

    // This path exists due to the symlinks created above.
    let too_long = path.join("x/y/".repeat(libc::PATH_MAX.try_into().unwrap()));

    let c_path = CString::new(too_long.into_os_string().as_bytes()).expect("CString::new failed");
    let r = unsafe { libc::realpath(c_path.as_ptr(), std::ptr::null_mut()) };
    let e = std::io::Error::last_os_error();

    // Cleanup after test.
    remove_file(&yx_sym).ok();
    remove_file(&xy_sym).ok();
    remove_dir(&x).ok();
    remove_dir(&y).ok();

    assert!(r.is_null());
    assert_eq!(e.raw_os_error(), Some(libc::ENAMETOOLONG));
    assert_eq!(e.kind(), ErrorKind::InvalidFilename);
}

#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn test_posix_fadvise() {
    use std::convert::TryInto;
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    let path = tmp().join("miri_test_libc_posix_fadvise.txt");
    // Cleanup before test
    remove_file(&path).ok();

    // Set up an open file
    let mut file = File::create(&path).unwrap();
    let bytes = b"Hello, World!\n";
    file.write(bytes).unwrap();

    // Test calling posix_fadvise on a file.
    let result = unsafe {
        libc::posix_fadvise(
            file.as_raw_fd(),
            0,
            bytes.len().try_into().unwrap(),
            libc::POSIX_FADV_DONTNEED,
        )
    };
    drop(file);
    remove_file(&path).unwrap();
    assert_eq!(result, 0);
}

#[cfg(any(target_os = "linux"))]
fn test_sync_file_range() {
    use std::fs::{remove_file, File};
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    let path = tmp().join("miri_test_libc_sync_file_range.txt");
    // Cleanup before test.
    remove_file(&path).ok();

    // Write to a file.
    let mut file = File::create(&path).unwrap();
    let bytes = b"Hello, World!\n";
    file.write(bytes).unwrap();

    // Test calling sync_file_range on the file.
    let result_1 = unsafe {
        libc::sync_file_range(
            file.as_raw_fd(),
            0,
            0,
            libc::SYNC_FILE_RANGE_WAIT_BEFORE
                | libc::SYNC_FILE_RANGE_WRITE
                | libc::SYNC_FILE_RANGE_WAIT_AFTER,
        )
    };
    drop(file);

    // Test calling sync_file_range on a file opened for reading.
    let file = File::open(&path).unwrap();
    let result_2 = unsafe {
        libc::sync_file_range(
            file.as_raw_fd(),
            0,
            0,
            libc::SYNC_FILE_RANGE_WAIT_BEFORE
                | libc::SYNC_FILE_RANGE_WRITE
                | libc::SYNC_FILE_RANGE_WAIT_AFTER,
        )
    };
    drop(file);

    remove_file(&path).unwrap();
    assert_eq!(result_1, 0);
    assert_eq!(result_2, 0);
}

fn test_mutex_libc_init_recursive() {
    unsafe {
        let mut attr: libc::pthread_mutexattr_t = std::mem::zeroed();
        assert_eq!(libc::pthread_mutexattr_init(&mut attr as *mut _), 0);
        assert_eq!(
            libc::pthread_mutexattr_settype(&mut attr as *mut _, libc::PTHREAD_MUTEX_RECURSIVE),
            0,
        );
        let mut mutex: libc::pthread_mutex_t = std::mem::zeroed();
        assert_eq!(libc::pthread_mutex_init(&mut mutex as *mut _, &mut attr as *mut _), 0);
        assert_eq!(libc::pthread_mutex_lock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_trylock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_trylock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_lock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), libc::EPERM);
        assert_eq!(libc::pthread_mutex_destroy(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutexattr_destroy(&mut attr as *mut _), 0);
    }
}

fn test_mutex_libc_init_normal() {
    unsafe {
        let mut mutexattr: libc::pthread_mutexattr_t = std::mem::zeroed();
        assert_eq!(
            libc::pthread_mutexattr_settype(&mut mutexattr as *mut _, 0x12345678),
            libc::EINVAL,
        );
        assert_eq!(
            libc::pthread_mutexattr_settype(&mut mutexattr as *mut _, libc::PTHREAD_MUTEX_NORMAL),
            0,
        );
        let mut mutex: libc::pthread_mutex_t = std::mem::zeroed();
        assert_eq!(libc::pthread_mutex_init(&mut mutex as *mut _, &mutexattr as *const _), 0);
        assert_eq!(libc::pthread_mutex_lock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_trylock(&mut mutex as *mut _), libc::EBUSY);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_trylock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_destroy(&mut mutex as *mut _), 0);
    }
}

fn test_mutex_libc_init_errorcheck() {
    unsafe {
        let mut mutexattr: libc::pthread_mutexattr_t = std::mem::zeroed();
        assert_eq!(
            libc::pthread_mutexattr_settype(
                &mut mutexattr as *mut _,
                libc::PTHREAD_MUTEX_ERRORCHECK,
            ),
            0,
        );
        let mut mutex: libc::pthread_mutex_t = std::mem::zeroed();
        assert_eq!(libc::pthread_mutex_init(&mut mutex as *mut _, &mutexattr as *const _), 0);
        assert_eq!(libc::pthread_mutex_lock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_trylock(&mut mutex as *mut _), libc::EBUSY);
        assert_eq!(libc::pthread_mutex_lock(&mut mutex as *mut _), libc::EDEADLK);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_trylock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), 0);
        assert_eq!(libc::pthread_mutex_unlock(&mut mutex as *mut _), libc::EPERM);
        assert_eq!(libc::pthread_mutex_destroy(&mut mutex as *mut _), 0);
    }
}

// Only linux provides PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP,
// libc for macOS just has the default PTHREAD_MUTEX_INITIALIZER.
#[cfg(target_os = "linux")]
fn test_mutex_libc_static_initializer_recursive() {
    let mutex = std::cell::UnsafeCell::new(libc::PTHREAD_RECURSIVE_MUTEX_INITIALIZER_NP);
    unsafe {
        assert_eq!(libc::pthread_mutex_lock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_trylock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_unlock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_unlock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_trylock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_lock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_unlock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_unlock(mutex.get()), 0);
        assert_eq!(libc::pthread_mutex_unlock(mutex.get()), libc::EPERM);
        assert_eq!(libc::pthread_mutex_destroy(mutex.get()), 0);
    }
}

// Testing the behavior of std::sync::RwLock does not fully exercise the pthread rwlock shims, we
// need to go a layer deeper and test the behavior of the libc functions, because
// std::sys::unix::rwlock::RWLock itself keeps track of write_locked and num_readers.
fn test_rwlock_libc_static_initializer() {
    let rw = std::cell::UnsafeCell::new(libc::PTHREAD_RWLOCK_INITIALIZER);
    unsafe {
        assert_eq!(libc::pthread_rwlock_rdlock(rw.get()), 0);
        assert_eq!(libc::pthread_rwlock_rdlock(rw.get()), 0);
        assert_eq!(libc::pthread_rwlock_unlock(rw.get()), 0);
        assert_eq!(libc::pthread_rwlock_tryrdlock(rw.get()), 0);
        assert_eq!(libc::pthread_rwlock_unlock(rw.get()), 0);
        assert_eq!(libc::pthread_rwlock_trywrlock(rw.get()), libc::EBUSY);
        assert_eq!(libc::pthread_rwlock_unlock(rw.get()), 0);

        assert_eq!(libc::pthread_rwlock_wrlock(rw.get()), 0);
        assert_eq!(libc::pthread_rwlock_tryrdlock(rw.get()), libc::EBUSY);
        assert_eq!(libc::pthread_rwlock_trywrlock(rw.get()), libc::EBUSY);
        assert_eq!(libc::pthread_rwlock_unlock(rw.get()), 0);

        assert_eq!(libc::pthread_rwlock_trywrlock(rw.get()), 0);
        assert_eq!(libc::pthread_rwlock_tryrdlock(rw.get()), libc::EBUSY);
        assert_eq!(libc::pthread_rwlock_trywrlock(rw.get()), libc::EBUSY);
        assert_eq!(libc::pthread_rwlock_unlock(rw.get()), 0);

        assert_eq!(libc::pthread_rwlock_destroy(rw.get()), 0);
    }
}

/// Test whether the `prctl` shim correctly sets the thread name.
///
/// Note: `prctl` exists only on Linux.
#[cfg(any(target_os = "linux"))]
fn test_prctl_thread_name() {
    use libc::c_long;
    use std::ffi::CString;
    unsafe {
        let mut buf = [255; 10];
        assert_eq!(
            libc::prctl(libc::PR_GET_NAME, buf.as_mut_ptr(), 0 as c_long, 0 as c_long, 0 as c_long),
            0,
        );
        // Rust runtime might set thread name, so we allow two options here.
        assert!(&buf[..10] == b"<unnamed>\0" || &buf[..5] == b"main\0");
        let thread_name = CString::new("hello").expect("CString::new failed");
        assert_eq!(
            libc::prctl(
                libc::PR_SET_NAME,
                thread_name.as_ptr(),
                0 as c_long,
                0 as c_long,
                0 as c_long,
            ),
            0,
        );
        let mut buf = [255; 6];
        assert_eq!(
            libc::prctl(libc::PR_GET_NAME, buf.as_mut_ptr(), 0 as c_long, 0 as c_long, 0 as c_long),
            0,
        );
        assert_eq!(b"hello\0", &buf);
        let long_thread_name = CString::new("01234567890123456789").expect("CString::new failed");
        assert_eq!(
            libc::prctl(
                libc::PR_SET_NAME,
                long_thread_name.as_ptr(),
                0 as c_long,
                0 as c_long,
                0 as c_long,
            ),
            0,
        );
        let mut buf = [255; 16];
        assert_eq!(
            libc::prctl(libc::PR_GET_NAME, buf.as_mut_ptr(), 0 as c_long, 0 as c_long, 0 as c_long),
            0,
        );
        assert_eq!(b"012345678901234\0", &buf);
    }
}

/// Tests whether each thread has its own `__errno_location`.
fn test_thread_local_errno() {
    #[cfg(target_os = "linux")]
    use libc::__errno_location;
    #[cfg(any(target_os = "macos", target_os = "freebsd"))]
    use libc::__error as __errno_location;

    unsafe {
        *__errno_location() = 0xBEEF;
        std::thread::spawn(|| {
            assert_eq!(*__errno_location(), 0);
            *__errno_location() = 0xBAD1DEA;
            assert_eq!(*__errno_location(), 0xBAD1DEA);
        })
        .join()
        .unwrap();
        assert_eq!(*__errno_location(), 0xBEEF);
    }
}

/// Tests whether clock support exists at all
#[cfg(any(target_os = "linux"))]
fn test_clocks() {
    let mut tp = std::mem::MaybeUninit::<libc::timespec>::uninit();
    let is_error = unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, tp.as_mut_ptr()) };
    assert_eq!(is_error, 0);
    let is_error = unsafe { libc::clock_gettime(libc::CLOCK_REALTIME_COARSE, tp.as_mut_ptr()) };
    assert_eq!(is_error, 0);
    let is_error = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, tp.as_mut_ptr()) };
    assert_eq!(is_error, 0);
    let is_error = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_COARSE, tp.as_mut_ptr()) };
    assert_eq!(is_error, 0);
}

fn test_posix_gettimeofday() {
    let mut tp = std::mem::MaybeUninit::<libc::timeval>::uninit();
    let tz = std::ptr::null_mut::<libc::timezone>();
    #[cfg(target_os = "macos")] // `tz` has a different type on macOS
    let tz = tz as *mut libc::c_void;
    let is_error = unsafe { libc::gettimeofday(tp.as_mut_ptr(), tz) };
    assert_eq!(is_error, 0);
    let tv = unsafe { tp.assume_init() };
    assert!(tv.tv_sec > 0);
    assert!(tv.tv_usec >= 0); // Theoretically this could be 0.

    // Test that non-null tz returns an error.
    let mut tz = std::mem::MaybeUninit::<libc::timezone>::uninit();
    let tz_ptr = tz.as_mut_ptr();
    #[cfg(target_os = "macos")] // `tz` has a different type on macOS
    let tz_ptr = tz_ptr as *mut libc::c_void;
    let is_error = unsafe { libc::gettimeofday(tp.as_mut_ptr(), tz_ptr) };
    assert_eq!(is_error, -1);
}

fn main() {
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    test_posix_fadvise();

    test_posix_gettimeofday();

    test_posix_realpath_alloc();
    test_posix_realpath_noalloc();
    test_posix_realpath_errors();

    #[cfg(any(target_os = "linux"))]
    test_sync_file_range();

    test_mutex_libc_init_recursive();
    test_mutex_libc_init_normal();
    test_mutex_libc_init_errorcheck();
    test_rwlock_libc_static_initializer();

    #[cfg(any(target_os = "linux"))]
    test_mutex_libc_static_initializer_recursive();

    #[cfg(any(target_os = "linux"))]
    test_prctl_thread_name();

    test_thread_local_errno();

    #[cfg(any(target_os = "linux"))]
    test_clocks();
}
