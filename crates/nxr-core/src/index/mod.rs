pub mod btree;
pub mod inverted;

use crate::config::Config;
use crate::error::NxrResult;

pub struct IndexManager {
    pub btree: btree::BPlusTree,
    pub inverted: inverted::InvertedIndex,
}

impl IndexManager {
    pub fn new(config: &Config) -> NxrResult<Self> {
        Ok(Self {
            btree: btree::BPlusTree::new(config.index.btree_order as usize),
            inverted: inverted::InvertedIndex::new(),
        })
    }

    pub fn fragmentation(&self) -> f32 {
        self.btree.fragmentation()
    }

    pub fn rebuild(&mut self) -> NxrResult<()> {
        self.btree.rebuild()
    }
}
