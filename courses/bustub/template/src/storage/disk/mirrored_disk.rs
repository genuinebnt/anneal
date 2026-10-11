//! A disk made of two disks that keep the same pages, so that one of them failing does not lose anything (RAID 1).

use std::collections::HashSet;
use std::io;
use std::sync::{Arc, Mutex};

use crate::common::config::{PageData, PageId};
use crate::storage::disk::disk_manager::DiskIo;

/// Two copies of every page. A write goes to both; a read comes from whichever copy is good. One disk may fail reads and writes for a while
/// (and then come back) but never both at the same moment; through all of it, a read returns the latest write that succeeded.
pub struct MirroredDisk {
    _disks: (),
}

impl MirroredDisk {
    /// A mirror over two disks that hold the same pages.
    pub fn new(primary: Arc<dyn DiskIo>, secondary: Arc<dyn DiskIo>) -> MirroredDisk {
        todo!("1a-c1: two disks that keep the same pages")
    }
}

impl DiskIo for MirroredDisk {
    fn read_page(&self, page_id: PageId, buf: &mut PageData) -> io::Result<()> {
        todo!("1a-c1: read from a copy that is good")
    }
    
    fn write_page(&self, page_id: PageId, data: &PageData) -> io::Result<()> {
        todo!("1a-c1: keep both copies")
    }
    
    fn delete_page(&self, page_id: PageId) {
        todo!("1a-c1: delete from both")
    }
}
