//! Which page of a heap file has room for a row.

pub struct FreeSpaceMap {
    // @begin 3c-c1
    n: usize,
    /// A binary tree of maxima over the pages; leaves start at `size`.
    tree: Vec<u32>,
    size: usize,
    //~ _fsm: (),
    // @end
}

impl FreeSpaceMap {
    /// A map for `pages` pages, none with free space.
    pub fn new(pages: usize) -> FreeSpaceMap {
        // @begin 3c-c1
        let size = pages.next_power_of_two().max(1);
        FreeSpaceMap { n: pages, tree: vec![0; 2 * size], size }
        //~ todo!("3c-c1: every page has no free space")
        // @end
    }

    /// The number of pages.
    pub fn pages(&self) -> usize {
        // @begin 3c-c1
        self.n
        //~ todo!("3c-c1: how many pages")
        // @end
    }

    /// Records `free` bytes of room for `page`.
    pub fn set(&mut self, page: usize, free: u32) {
        // @begin 3c-c1
        let mut i = self.size + page;
        self.tree[i] = free;
        while i > 1 {
            i /= 2;
            self.tree[i] = self.tree[2 * i].max(self.tree[2 * i + 1]);
        }
        //~ todo!("3c-c1: store the value and fix the maxima on the way up")
        // @end
    }

    /// The room recorded for `page`.
    pub fn get(&self, page: usize) -> u32 {
        // @begin 3c-c1
        self.tree[self.size + page]
        //~ todo!("3c-c1: the stored value")
        // @end
    }

    /// The lowest page with at least `need` free bytes.
    pub fn find(&self, need: u32) -> Option<usize> {
        // @begin 3c-c1
        if self.n == 0 || self.tree[1] < need {
            return None;
        }
        let mut i = 1;
        while i < self.size {
            i = if self.tree[2 * i] >= need { 2 * i } else { 2 * i + 1 };
        }
        let page = i - self.size;
        if page < self.n {
            Some(page)
        } else {
            None
        }
        //~ todo!("3c-c1: descend, going left whenever the left half has enough room")
        // @end
    }
}
