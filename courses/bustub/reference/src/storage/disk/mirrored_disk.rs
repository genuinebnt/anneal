//! A disk made of two disks that keep the same pages, so that one of them failing does not lose anything (RAID 1).

use std::collections::HashSet;
use std::io;
use std::sync::{Arc, Mutex};

use crate::common::config::{PageData, PageId};
use crate::storage::disk::disk_manager::DiskIo;

/// Two copies of every page. A write goes to both; a read comes from whichever copy is good. One disk may fail reads and writes for a while
/// (and then come back) but never both at the same moment; through all of it, a read returns the latest write that succeeded.
pub struct MirroredDisk {
    // @begin 1a-c1
    primary: Arc<dyn DiskIo>,
    secondary: Arc<dyn DiskIo>,
    /// Pages one disk missed a write of (it failed): its copy is old until repaired. (primary, secondary).
    stale: Mutex<(HashSet<PageId>, HashSet<PageId>)>,
    //~ _disks: (),
    // @end
}

impl MirroredDisk {
    /// A mirror over two disks that hold the same pages.
    pub fn new(primary: Arc<dyn DiskIo>, secondary: Arc<dyn DiskIo>) -> MirroredDisk {
        // @begin 1a-c1
        MirroredDisk { primary, secondary, stale: Mutex::new((HashSet::new(), HashSet::new())) }
        //~ todo!("1a-c1: two disks that keep the same pages")
        // @end
    }
}

impl DiskIo for MirroredDisk {
    // @begin 1a-c1
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        let (p_stale, s_stale) = {
            let st = self.stale.lock().unwrap();
            (st.0.contains(&page_id), st.1.contains(&page_id))
        };
        // try the copies that are current, the primary first
        let order: [(bool, &Arc<dyn DiskIo>, bool); 2] = [(true, &self.primary, p_stale), (false, &self.secondary, s_stale)];
        let mut last_err = None;
        for (is_primary, disk, stale) in order {
            if stale {
                continue;
            }
            match disk.read_page(page_id, buf) {
                Ok(()) => {
                    // repair the other copy if it was behind
                    let other_stale = if is_primary { s_stale } else { p_stale };
                    if other_stale {
                        let other = if is_primary { &self.secondary } else { &self.primary };
                        if other.write_page(page_id, buf).is_ok() {
                            let mut st = self.stale.lock().unwrap();
                            if is_primary { st.1.remove(&page_id) } else { st.0.remove(&page_id) };
                        }
                    }
                    return Ok(());
                }
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or_else(|| io::Error::other("no current copy of the page")))
    }

    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        let a = self.primary.write_page(page_id, data);
        let b = self.secondary.write_page(page_id, data);
        let mut st = self.stale.lock().unwrap();
        match (&a, &b) {
            (Ok(()), Ok(())) => {
                st.0.remove(&page_id);
                st.1.remove(&page_id);
                Ok(())
            }
            (Ok(()), Err(_)) => {
                st.0.remove(&page_id);
                st.1.insert(page_id);
                Ok(())
            }
            (Err(_), Ok(())) => {
                st.1.remove(&page_id);
                st.0.insert(page_id);
                Ok(())
            }
            (Err(e), Err(_)) => Err(io::Error::new(e.kind(), "both disks failed the write")),
        }
    }

    fn delete_page(&self, page_id: PageId) {
        self.primary.delete_page(page_id);
        self.secondary.delete_page(page_id);
        let mut st = self.stale.lock().unwrap();
        st.0.remove(&page_id);
        st.1.remove(&page_id);
    }
    //~ fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
    //~     todo!("1a-c1: read from a copy that is good")
    //~ }
    //~
    //~ fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
    //~     todo!("1a-c1: keep both copies")
    //~ }
    //~
    //~ fn delete_page(&self, page_id: PageId) {
    //~     todo!("1a-c1: delete from both")
    //~ }
    // @end
}
