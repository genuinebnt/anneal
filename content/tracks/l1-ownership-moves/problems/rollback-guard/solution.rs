pub fn with_rollback(v: &mut Vec<i32>, f: impl FnOnce(&mut Vec<i32>)) {
    struct Guard<'a> {
        v: &'a mut Vec<i32>,
        len: usize,
        armed: bool,
    }

    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            if self.armed {
                self.v.truncate(self.len);
            }
        }
    }

    let len = v.len();
    let mut guard = Guard {
        v,
        len,
        armed: true,
    };
    f(guard.v);
    // Reached only if `f` returned: keep its pushes.
    guard.armed = false;
}
