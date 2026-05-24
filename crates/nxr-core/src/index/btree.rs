use crate::error::NxrResult;
use std::fmt;

#[derive(Debug, Clone)]
struct LeafEntry {
    key: Vec<u8>,
    value_ids: Vec<u64>,
}

#[derive(Debug, Clone)]
struct LeafNode {
    entries: Vec<LeafEntry>,
    next_leaf: Option<usize>,
}

#[derive(Debug, Clone)]
struct InternalNode {
    keys: Vec<Vec<u8>>,
    children: Vec<usize>,
}

#[derive(Debug, Clone)]
enum BpNode {
    Leaf(LeafNode),
    Internal(InternalNode),
}

pub struct BPlusTree {
    arena: Vec<BpNode>,
    root: Option<usize>,
    order: usize,
    min_keys: usize,
    len: usize,
}

impl BPlusTree {
    pub fn new(order: usize) -> Self {
        let min_keys = if order > 2 { (order + 1) / 2 } else { 1 };
        Self {
            arena: Vec::new(),
            root: None,
            order,
            min_keys,
            len: 0,
        }
    }

    fn alloc(&mut self, node: BpNode) -> usize {
        let id = self.arena.len();
        self.arena.push(node);
        id
    }

    fn new_leaf(&mut self) -> usize {
        self.alloc(BpNode::Leaf(LeafNode {
            entries: Vec::new(),
            next_leaf: None,
        }))
    }

    fn new_internal(&mut self, keys: Vec<Vec<u8>>, children: Vec<usize>) -> usize {
        self.alloc(BpNode::Internal(InternalNode { keys, children }))
    }

    /// Find the leaf where key should be inserted/searched
    fn find_leaf(&self, key: &[u8]) -> Option<usize> {
        let mut node = self.root?;
        loop {
            match &self.arena[node] {
                BpNode::Leaf(_) => return Some(node),
                BpNode::Internal(internal) => {
                    let pos = internal.keys.partition_point(|k| k.as_slice() <= key);
                    node = internal.children[pos];
                }
            }
        }
    }

    pub fn insert(&mut self, key: &[u8], value_id: u64) {
        if self.root.is_none() {
            let leaf = self.new_leaf();
            self.root = Some(leaf);
        }

        let leaf_id = self.find_leaf(key).unwrap();
        let result = self.insert_into_leaf(leaf_id, key.to_vec(), value_id);

        if let Some((new_key, new_leaf_id)) = result {
            self.insert_into_parent(leaf_id, new_key, new_leaf_id);
        }
    }

    fn insert_into_leaf(
        &mut self,
        leaf_id: usize,
        key: Vec<u8>,
        value_id: u64,
    ) -> Option<(Vec<u8>, usize)> {
        // Extract needed data before mutable arena access
        let (should_split, split_at, new_key, new_entries, next_leaf) = {
            let leaf = match &mut self.arena[leaf_id] {
                BpNode::Leaf(leaf) => leaf,
                _ => unreachable!(),
            };

            let pos = leaf.entries.partition_point(|e| e.key.as_slice() < key.as_slice());

            if pos < leaf.entries.len() && leaf.entries[pos].key == key {
                leaf.entries[pos].value_ids.push(value_id);
                self.len += 1;
                return None;
            }

            leaf.entries.insert(
                pos,
                LeafEntry {
                    key,
                    value_ids: vec![value_id],
                },
            );
            self.len += 1;

            if leaf.entries.len() > self.order {
                let split_at = leaf.entries.len() / 2;
                let new_entries = leaf.entries.split_off(split_at);
                let new_key = new_entries[0].key.clone();
                let next = leaf.next_leaf;
                leaf.next_leaf = None;
                (true, split_at, new_key, new_entries, next)
            } else {
                (false, 0, Vec::new(), Vec::new(), None)
            }
        };

        if should_split {
            let new_leaf_id = self.new_leaf();
            if let BpNode::Leaf(new_leaf) = &mut self.arena[new_leaf_id] {
                new_leaf.entries = new_entries;
                new_leaf.next_leaf = next_leaf;
            }
            if let BpNode::Leaf(leaf) = &mut self.arena[leaf_id] {
                leaf.next_leaf = Some(new_leaf_id);
            }
            Some((new_key, new_leaf_id))
        } else {
            None
        }
    }

    fn insert_into_parent(
        &mut self,
        left_id: usize,
        split_key: Vec<u8>,
        right_id: usize,
    ) {
        let Some(root_id) = self.root else { return };

        // If root is the leaf, create new root
        if root_id == left_id {
            let new_root = self.new_internal(vec![split_key], vec![left_id, right_id]);
            self.root = Some(new_root);
            return;
        }

        // Find parent
        let parent_id = self.find_parent(root_id, left_id);
        let Some(parent_id) = parent_id else { return };

        let result = self.insert_into_internal(parent_id, split_key, right_id);
        if let Some((new_key, new_child_id)) = result {
            self.insert_into_parent(parent_id, new_key, new_child_id);
        }
    }

    fn insert_into_internal(
        &mut self,
        node_id: usize,
        key: Vec<u8>,
        child_id: usize,
    ) -> Option<(Vec<u8>, usize)> {
        let internal = match &mut self.arena[node_id] {
            BpNode::Internal(internal) => internal,
            _ => unreachable!(),
        };

        let pos = internal.keys.partition_point(|k| k.as_slice() < key.as_slice());
        internal.keys.insert(pos, key);
        internal.children.insert(pos + 1, child_id);

        // Split if over capacity
        if internal.keys.len() > self.order {
            let split_at = internal.keys.len() / 2;
            let mid_key = internal.keys[split_at].clone();

            let right_keys: Vec<Vec<u8>> = internal.keys.split_off(split_at + 1);
            let right_children: Vec<usize> = internal.children.split_off(split_at + 1);

            // Remove mid_key from left node
            internal.keys.pop();

            let new_node_id = self.new_internal(right_keys, right_children);
            Some((mid_key, new_node_id))
        } else {
            None
        }
    }

    fn find_parent(&self, current_id: usize, target_id: usize) -> Option<usize> {
        let node = &self.arena[current_id];
        match node {
            BpNode::Leaf(_) => None,
            BpNode::Internal(internal) => {
                if internal.children.contains(&target_id) {
                    Some(current_id)
                } else {
                    for &child in &internal.children {
                        if let Some(found) = self.find_parent(child, target_id) {
                            return Some(found);
                        }
                    }
                    None
                }
            }
        }
    }

    pub fn search(&self, key: &[u8]) -> Option<&Vec<u64>> {
        let leaf_id = self.find_leaf(key)?;
        match &self.arena[leaf_id] {
            BpNode::Leaf(leaf) => {
                let pos = leaf.entries.partition_point(|e| e.key.as_slice() < key);
                if pos < leaf.entries.len() && leaf.entries[pos].key.as_slice() == key {
                    Some(&leaf.entries[pos].value_ids)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn range_scan(&self, start: &[u8], end: &[u8]) -> Vec<&Vec<u64>> {
        let mut results = Vec::new();
        let Some(leaf_id) = self.find_leaf(start) else { return results };
        let mut current = leaf_id;

        loop {
            let leaf = match &self.arena[current] {
                BpNode::Leaf(leaf) => leaf,
                _ => break,
            };

            for entry in &leaf.entries {
                if entry.key.as_slice() > end {
                    return results;
                }
                if entry.key.as_slice() >= start {
                    results.push(&entry.value_ids);
                }
            }

            match leaf.next_leaf {
                Some(next) => current = next,
                None => break,
            }
        }

        results
    }

    pub fn delete(&mut self, key: &[u8], value_id: u64) {
        let Some(leaf_id) = self.find_leaf(key) else { return };
        let leaf = match &mut self.arena[leaf_id] {
            BpNode::Leaf(leaf) => leaf,
            _ => return,
        };

        let pos = leaf.entries.partition_point(|e| e.key.as_slice() < key);
        if pos < leaf.entries.len() && leaf.entries[pos].key.as_slice() == key {
            leaf.entries[pos].value_ids.retain(|id| *id != value_id);
            if leaf.entries[pos].value_ids.is_empty() {
                leaf.entries.remove(pos);
            }
            self.len = self.len.saturating_sub(1);
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Return fragmentation ratio (0.0 = none, 1.0 = max)
    pub fn fragmentation(&self) -> f32 {
        if self.arena.is_empty() {
            return 0.0;
        }
        let total_capacity = self.arena.len() * self.order;
        if total_capacity == 0 {
            return 0.0;
        }
        let total_entries: usize = self
            .arena
            .iter()
            .map(|n| match n {
                BpNode::Leaf(leaf) => leaf.entries.len(),
                BpNode::Internal(internal) => internal.keys.len(),
            })
            .sum();
        1.0 - (total_entries as f32 / total_capacity as f32)
    }

    /// Rebuild tree to optimal structure
    pub fn rebuild(&mut self) -> NxrResult<()> {
        // Collect all entries from leaves via linked list
        let mut all_entries: Vec<LeafEntry> = Vec::new();
        if let Some(root_id) = self.root {
            // Find leftmost leaf
            let mut node = root_id;
            loop {
                match &self.arena[node] {
                    BpNode::Leaf(leaf) => {
                        all_entries.extend(leaf.entries.iter().cloned());
                        break;
                    }
                    BpNode::Internal(internal) => {
                        node = internal.children[0];
                    }
                }
            }
            // Walk leaf chain
            loop {
                match &self.arena[node] {
                    BpNode::Leaf(leaf) => {
                        if let Some(next) = leaf.next_leaf {
                            if let BpNode::Leaf(next_leaf) = &self.arena[next] {
                                all_entries.extend(next_leaf.entries.iter().cloned());
                            }
                            node = next;
                        } else {
                            break;
                        }
                    }
                    _ => break,
                }
            }
        }

        if all_entries.is_empty() {
            return Ok(());
        }

        // Rebuild from scratch
        all_entries.sort_by(|a, b| a.key.cmp(&b.key));
        self.arena.clear();
        self.root = None;
        self.len = 0;

        // Bulk insert in order
        for entry in all_entries {
            for &vid in &entry.value_ids {
                self.insert(&entry.key, vid);
            }
        }

        Ok(())
    }
}

impl fmt::Debug for BPlusTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "BPlusTree(order={}, nodes={}, entries={})",
            self.order,
            self.arena.len(),
            self.len
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_search() {
        let mut tree = BPlusTree::new(4);
        tree.insert(b"alice", 1);
        tree.insert(b"bob", 2);
        tree.insert(b"charlie", 3);

        assert_eq!(tree.search(b"alice"), Some(&vec![1]));
        assert_eq!(tree.search(b"bob"), Some(&vec![2]));
        assert_eq!(tree.search(b"dave"), None);
    }

    #[test]
    fn test_multi_value() {
        let mut tree = BPlusTree::new(4);
        tree.insert(b"key1", 1);
        tree.insert(b"key1", 2);
        tree.insert(b"key1", 3);

        let values = tree.search(b"key1").unwrap();
        assert_eq!(values.len(), 3);
    }

    #[test]
    fn test_range_scan() {
        let mut tree = BPlusTree::new(4);
        for i in 0..20u64 {
            let key = format!("key_{:03}", i);
            tree.insert(key.as_bytes(), i);
        }

        let results = tree.range_scan(b"key_005", b"key_010");
        assert!(!results.is_empty());
    }

    #[test]
    fn test_large_insert() {
        let mut tree = BPlusTree::new(16);
        for i in 0..1000u64 {
            let key = format!("user_{}", i);
            tree.insert(key.as_bytes(), i);
        }
        assert_eq!(tree.len(), 1000);
        assert!(tree.fragmentation() < 1.0);
    }

    #[test]
    fn test_delete() {
        let mut tree = BPlusTree::new(4);
        tree.insert(b"key1", 1);
        tree.insert(b"key1", 2);
        tree.delete(b"key1", 1);
        assert_eq!(tree.search(b"key1"), Some(&vec![2]));
    }
}
