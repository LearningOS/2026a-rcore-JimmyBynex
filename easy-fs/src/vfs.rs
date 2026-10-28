use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Get the inode id of this inode
    pub fn inode_id(&self) -> u32 {
        self.fs
            .lock()
            .get_inode_id(self.block_id as u32, self.block_offset)
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }
    /// Create inode under current inode by name
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// Whether this inode is a directory
    pub fn is_dir(&self) -> bool {
        self.read_disk_inode(|disk_inode| disk_inode.is_dir())
    }
    /// Number of names in the root directory that point to this inode
    pub fn nlink(&self) -> u32 {
        let inode_id = self.inode_id();
        EasyFileSystem::root_inode(&self.fs).count_links(inode_id)
    }
    /// Count the entries under this directory that point to `inode_id`
    pub fn count_links(&self, inode_id: u32) -> u32 {
        let _fs = self.fs.lock();
        self.read_disk_inode(|dir_inode| {
            let file_count = (dir_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::empty();
            let mut count = 0;
            for i in 0..file_count {
                dir_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device);
                if dirent.inode_id() == inode_id {
                    count += 1;
                }
            }
            count
        })
    }
    /// Add a new name `new_name` under this directory for the file named `old_name`.
    /// No inode or data is copied: the new entry reuses the old inode id.
    /// Returns false if `old_name` does not exist or `new_name` already exists.
    pub fn link(&self, old_name: &str, new_name: &str) -> bool {
        let mut fs = self.fs.lock();
        let old_inode_id = match self.read_disk_inode(|dir_inode| self.find_inode_id(old_name, dir_inode)) {
            Some(inode_id) => inode_id,
            None => return false,
        };
        if self
            .read_disk_inode(|dir_inode| self.find_inode_id(new_name, dir_inode))
            .is_some()
        {
            return false;
        }
        // append an entry, the same way as create
        self.modify_disk_inode(|dir_inode| {
            let file_count = (dir_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            self.increase_size(new_size as u32, dir_inode, &mut fs);
            let dirent = DirEntry::new(new_name, old_inode_id);
            dir_inode.write_at(file_count * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
        });
        block_cache_sync_all();
        true
    }
    /// Remove the entry `name` under this directory.
    /// If it was the last name of its inode, also free the inode and its data blocks.
    /// Returns false if `name` does not exist.
    pub fn unlink(&self, name: &str) -> bool {
        let mut fs = self.fs.lock();
        // Step 1: take the entry out of this directory.
        // Returns (inode id of the removed entry, how many names it still has).
        let removed = self.modify_disk_inode(|dir_inode| {
            let file_count = (dir_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::empty();
            let mut target = None;
            for i in 0..file_count {
                dir_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device);
                if dirent.name() == name {
                    target = Some((i, dirent.inode_id()));
                    break;
                }
            }
            let (index, inode_id) = target?;
            // fill the hole with the last entry, then drop the last slot
            let last = file_count - 1;
            if index != last {
                dir_inode.read_at(last * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device);
                dir_inode.write_at(index * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
            }
            let old_blocks = dir_inode.data_blocks();
            dir_inode.size = (last * DIRENT_SZ) as u32;
            // the last data block of the directory became empty: give it back,
            // otherwise a later increase_size would overwrite its block id and leak it.
            // (Only direct blocks: a directory needs over 448 entries to go past them.)
            if dir_inode.data_blocks() < old_blocks {
                let block_index = dir_inode.data_blocks() as usize;
                assert!(block_index < dir_inode.direct.len());
                fs.dealloc_data(dir_inode.direct[block_index]);
                dir_inode.direct[block_index] = 0;
            }
            // how many names still point to this inode
            let mut remaining = 0;
            for i in 0..last {
                dir_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device);
                if dirent.inode_id() == inode_id {
                    remaining += 1;
                }
            }
            Some((inode_id, remaining))
        });
        let Some((inode_id, remaining)) = removed else {
            return false;
        };
        // Step 2: that was its last name, free the inode and everything it owns.
        // Must run after step 1 returns: the freed inode may live in the same block
        // as this directory's inode, and step 1 holds that block's cache lock.
        if remaining == 0 {
            let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
            let data_blocks = get_block_cache(block_id as usize, Arc::clone(&self.block_device))
                .lock()
                .modify(block_offset, |disk_inode: &mut DiskInode| {
                    disk_inode.clear_size(&self.block_device)
                });
            for data_block in data_blocks {
                fs.dealloc_data(data_block);
            }
            fs.dealloc_inode(inode_id);
        }
        block_cache_sync_all();
        true
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
}
