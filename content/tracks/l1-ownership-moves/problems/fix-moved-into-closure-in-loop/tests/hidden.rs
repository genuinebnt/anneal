use solution::*;

#[test]
fn big() {
    check!(r#"data = 0..10000, n = 4"#, sum_everywhere((0..10_000).collect(), 4), vec![49_995_000; 4]);
}

#[test]
fn empty_data() {
    check!(r#"data = [], n = 3"#, sum_everywhere(vec![], 3), vec![0, 0, 0]);
}

#[test]
fn single_value() {
    check!(r#"data = [9], n = 2"#, sum_everywhere(vec![9], 2), vec![9, 9]);
}

#[test]
fn uneven_split() {
    check!(r#"data = [1, 2, 3, 4, 5], n = 2"#, sum_everywhere(vec![1, 2, 3, 4, 5], 2), vec![15, 15]);
}

#[test]
fn sixteen_threads() {
    check!(r#"data = [1, 1, 1], n = 16"#, sum_everywhere(vec![1, 1, 1], 16), vec![3; 16]);
}

#[test]
fn large_values() {
    check!(r#"data = [u64::MAX / 4; 2], n = 2"#, sum_everywhere(vec![u64::MAX / 4; 2], 2), vec![u64::MAX / 4 * 2; 2]);
}

#[test]
fn empty_no_threads() {
    check!(r#"data = [], n = 0"#, sum_everywhere(vec![], 0), Vec::<u64>::new());
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(1113);
    for _ in 0..100 {
        let len = rng.below(10);
        let data: Vec<u64> = rng.vec(len, 0, 1_000_000);
        let n = rng.below(5);
        let want = vec![data.iter().sum::<u64>(); n];
        check!(format!("data = {data:?}, n = {n}"), sum_everywhere(data.clone(), n), want);
    }
}

// Counts allocations of 1 MiB or more on every thread, so a copy made inside a worker is seen too. The other
// tests in this binary only make small allocations, so running them in parallel doesn't change the count.
mod big_allocs {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub static BIG: AtomicUsize = AtomicUsize::new(0);
    const LIMIT: usize = 1 << 20;

    pub struct Counting;

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            if layout.size() >= LIMIT {
                BIG.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            if new_size >= LIMIT {
                BIG.fetch_add(1, Ordering::Relaxed);
            }
            unsafe { System.realloc(ptr, layout, new_size) }
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;
}

#[test]
fn data_not_copied_per_thread() {
    use std::sync::atomic::Ordering;
    let data: Vec<u64> = (0..1_000_000).collect();
    let before = big_allocs::BIG.load(Ordering::Relaxed);
    let sums = sum_everywhere(data, 4);
    let copies = big_allocs::BIG.load(Ordering::Relaxed) - before;
    // One copy in total (say, into an `Arc<[u64]>`) is fine; one per thread isn't.
    check!("data = 0..1000000 (8 MB), n = 4: sums, and at most one copy of the data", (sums, copies <= 1), (vec![499_999_500_000; 4], true));
}
