use {
    memmap2::MmapRaw,
    perf_event_open_sys::bindings::perf_event_mmap_page,
    std::{ptr, slice, sync::atomic::Ordering},
};

pub struct RingBufferAux {
    mmap: MmapRaw,
    aux_area: MmapRaw,
    size: usize,
}

impl RingBufferAux {
    pub fn new(mmap: MmapRaw, aux_area: MmapRaw) -> Self {
        let size = unsafe {
            ptr::read(ptr::addr_of!(
                (*(mmap.as_mut_ptr() as *mut perf_event_mmap_page)).aux_size
            ))
        } as usize;

        Self {
            mmap,
            aux_area,
            size,
        }
    }

    fn page(&self) -> *mut perf_event_mmap_page {
        self.mmap.as_mut_ptr() as *mut _
    }

    pub fn next_data<F: FnOnce(&[u8]) -> usize>(&mut self, callback: F) -> usize {
        let header = self.page();

        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        // - aux_tail is only written by the user side so it is safe to do a non-atomic
        //   read here.
        let tail = unsafe { ptr::read(ptr::addr_of!((*header).aux_tail)) } as usize;

        // ATOMICS:
        // - The acquire load here syncronizes with the release store in the kernel and
        //   ensures that all the data written to the ring buffer before aux_head is
        //   visible to this thread.
        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page.
        let head =
            unsafe { atomic_load(ptr::addr_of!((*header).aux_head), Ordering::Acquire) } as usize;

        if tail == head {
            return 0;
        }

        // panic if buffer is >90% full
        if head - tail > ((self.size * 10) / 9) {
            panic!("Ring buffer exceeded >90% capacity");
        }

        // head and tail constantly increase, need to wrap them to index the ring buffer
        let wrapped_head = head % self.size;
        let wrapped_tail = tail % self.size;

        let consumed = if wrapped_head > wrapped_tail {
            // single slice from tail to head
            let slice = unsafe {
                slice::from_raw_parts(
                    self.aux_area.as_mut_ptr().add(wrapped_tail),
                    wrapped_head - wrapped_tail,
                )
            };

            callback(slice)
        } else {
            // head is *less* than tail

            let mut buf = Vec::with_capacity((self.size - wrapped_tail) + wrapped_head);

            // so first slice goes from tail to the end of the buffer
            buf.extend_from_slice(unsafe {
                slice::from_raw_parts(
                    self.aux_area.as_mut_ptr().add(wrapped_tail),
                    self.size - wrapped_tail,
                )
            });

            // and the second slice goes from the beginning to the head
            buf.extend_from_slice(unsafe {
                slice::from_raw_parts(self.aux_area.as_mut_ptr(), wrapped_head)
            });

            callback(&buf)
        };

        // ATOMICS:
        // - The release store here prevents the compiler from re-ordering any reads
        //   past the store to aux_tail.
        // SAFETY:
        // - page points to a valid instance of perf_event_mmap_page
        unsafe {
            atomic_store(
                ptr::addr_of!((*header).aux_tail),
                u64::try_from(tail + consumed).unwrap(),
                Ordering::Release,
            );
        }

        consumed
    }
}

macro_rules! assert_same_size {
    ($a:ty, $b:ty) => {{
        if false {
            let _assert_same_size: [u8; ::std::mem::size_of::<$b>()] =
                [0u8; ::std::mem::size_of::<$a>()];
        }
    }};
}

trait Atomic: Sized + Copy {
    type Atomic;

    unsafe fn store(ptr: *const Self, val: Self, order: Ordering);
    unsafe fn load(ptr: *const Self, order: Ordering) -> Self;
}

macro_rules! impl_atomic {
    ($base:ty, $atomic:ty) => {
        impl Atomic for $base {
            type Atomic = $atomic;

            unsafe fn store(ptr: *const Self, val: Self, order: Ordering) {
                assert_same_size!(Self, Self::Atomic);

                let ptr = ptr as *const Self::Atomic;
                (*ptr).store(val, order)
            }

            unsafe fn load(ptr: *const Self, order: Ordering) -> Self {
                assert_same_size!(Self, Self::Atomic);

                let ptr = ptr as *const Self::Atomic;
                (*ptr).load(order)
            }
        }
    };
}

impl_atomic!(u64, std::sync::atomic::AtomicU64);
impl_atomic!(u32, std::sync::atomic::AtomicU32);
impl_atomic!(u16, std::sync::atomic::AtomicU16);
impl_atomic!(i64, std::sync::atomic::AtomicI64);

/// Do an atomic write to the value stored at `ptr`.
///
/// # Safety
/// - `ptr` must be valid for writes.
/// - `ptr` must be properly aligned.
unsafe fn atomic_store<T: Atomic>(ptr: *const T, val: T, order: Ordering) {
    T::store(ptr, val, order)
}

/// Perform an atomic read from the value stored at `ptr`.
///
/// # Safety
/// - `ptr` must be valid for reads.
/// - `ptr` must be properly aligned.
unsafe fn atomic_load<T: Atomic>(ptr: *const T, order: Ordering) -> T {
    T::load(ptr, order)
}
