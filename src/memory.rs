//! Return freed native document pages to glibc after native widgets are retired.

/// GTK releases text lines as many small allocations. Long-lived font, input
/// and renderer allocations can strand the resulting free pages in the heap.
/// Wait until GObject disposal/finalization has finished, then reclaim those
/// pages on a worker. Coalesce a retiring widget tree and allow its
/// replacement frame to settle.
/// No GTK objects cross threads and there is no permanent timer.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
pub fn reclaim_after_retirement() {
    use gtk::{gio, glib};
    thread_local! {
        static QUEUED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if QUEUED.replace(true) {
        return;
    }
    glib::timeout_add_local_once(std::time::Duration::from_millis(100), || {
        QUEUED.set(false);
        drop(gio::spawn_blocking(|| {
            unsafe extern "C" {
                fn malloc_trim(pad: usize) -> std::ffi::c_int;
            }
            // SAFETY: glibc's malloc_trim is thread-safe, takes no pointers and
            // only releases allocator-owned free pages. A zero return is fine.
            unsafe { malloc_trim(0) };
        }));
    });
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
pub fn reclaim_after_retirement() {}
