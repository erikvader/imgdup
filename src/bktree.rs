use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use crate::heap::{self, Heap, HeapBuilder, Ref};

use self::hamming::{Distance, Hamming};

mod hamming;

type Timestamp = u64;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct BKValue {
    timestamp: Timestamp,
    source: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct BKNode {
    hash: Hamming,
    value: Option<BKValue>,
    children: HashMap<Distance, Ref>,
}

pub struct BKTree {
    db: Heap<BKNode>,
}

impl BKTree {
    pub fn from_file(file: impl AsRef<Path>) -> heap::Result<Self> {
        let db = HeapBuilder::new().from_file(file)?;
        Ok(Self::new(db))
    }

    pub fn in_memory() -> heap::Result<Self> {
        Ok(Self::new(Heap::new_in_memory()?))
    }

    fn new(db: Heap<BKNode>) -> Self {
        Self { db }
    }

    pub fn close(self) -> heap::Result<()> {
        self.db.close()
    }

    pub fn add(&mut self, hash: Hamming, value: BKValue) -> heap::Result<()> {
        if self.db.root().is_null() {
            let root = self.db.allocate(BKNode::new(hash, value))?;
            self.db.set_root(root);
        } else {
            let mut cur_ref = self.db.root();
            loop {
                let cur_node = self.db.deref(cur_ref)?.expect("should have a value");
                let dist = cur_node.hash.distance_to(hash);

                if let Some(&child_ref) = cur_node.children.get(&dist) {
                    cur_ref = child_ref;
                } else {
                    let new_ref =
                        self.db.allocate_local(cur_ref, BKNode::new(hash, value))?;
                    let cur_node = self
                        .db
                        .deref_mut(cur_ref)?
                        .expect("the previous deref worked");

                    cur_node.children.insert(dist, new_ref);
                    break;
                }
            }
        }

        self.db.checkpoint()?;
        Ok(())
    }

    pub fn search<F>(
        &mut self,
        hash: Hamming,
        max_dist: Distance,
        mut visit: F,
    ) -> heap::Result<()>
    where
        F: FnMut(Hamming, &BKValue),
    {
        todo!()
    }

    pub fn remove_any_of<P>(&mut self, these: &HashSet<P>) -> heap::Result<()>
    where
        P: std::borrow::Borrow<Path> + Eq + std::hash::Hash,
    {
        self.for_each_internal(
            |node| {
                node.value
                    .as_ref()
                    .map_or(false, |value| these.contains(&value.source))
            },
            |node| node.value = None,
        )
    }

    pub fn for_each<F>(&mut self, mut visit: F) -> heap::Result<()>
    where
        F: FnMut(Hamming, &BKValue),
    {
        self.for_each_internal(
            |node| {
                if let Some(value) = &node.value {
                    visit(node.hash, value);
                }
                false
            },
            |_| (),
        )
    }

    fn for_each_internal<F, M>(
        &mut self,
        mut filter: F,
        mut modifier: M,
    ) -> heap::Result<()>
    where
        F: FnMut(&BKNode) -> bool,
        M: FnMut(&mut BKNode),
    {
        let mut stack = Vec::new();
        if !self.db.root().is_null() {
            stack.push(self.db.root());
        }

        while let Some(cur_ref) = stack.pop() {
            let cur_node = self.db.deref(cur_ref)?.expect("should have a value");
            stack.extend(cur_node.children.values());

            if filter(&cur_node) {
                let cur_node =
                    self.db.deref_mut(cur_ref)?.expect("previous deref worked");
                modifier(cur_node);
            }
        }

        Ok(())
    }
}

impl BKNode {
    fn new(hash: Hamming, value: BKValue) -> Self {
        Self {
            hash,
            value: Some(value),
            children: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn value(path: &str) -> BKValue {
        BKValue {
            timestamp: 0,
            source: path.into(),
        }
    }

    fn contents(tree: &mut BKTree) -> heap::Result<Vec<(Hamming, String)>> {
        let mut all = Vec::new();
        tree.for_each(|ham, val| {
            all.push((
                ham,
                val.source.clone().into_os_string().into_string().unwrap(),
            ))
        })?;
        all.sort();
        Ok(all)
    }

    #[test]
    fn add() -> heap::Result<()> {
        let mut tree = BKTree::in_memory()?;
        tree.add(Hamming(0b101), value("5_1"))?;
        tree.add(Hamming(0b101), value("5_2"))?;
        tree.add(Hamming(0b100), value("4"))?;

        let all = contents(&mut tree)?;

        assert_eq!(
            vec![
                (Hamming(0b100), "4".to_string()),
                (Hamming(0b101), "5_1".to_string()),
                (Hamming(0b101), "5_2".to_string()),
            ],
            all
        );

        Ok(())
    }

    #[test]
    fn remove() -> heap::Result<()> {
        let mut tree = BKTree::in_memory()?;
        tree.add(Hamming(0b101), value("5_1"))?;
        tree.add(Hamming(0b101), value("5_2"))?;
        tree.add(Hamming(0b100), value("4"))?;

        let rem: HashSet<PathBuf> = HashSet::from(["5_1".into()]);
        tree.remove_any_of(&rem)?;

        let all = contents(&mut tree)?;

        assert_eq!(
            vec![
                (Hamming(0b100), "4".to_string()),
                (Hamming(0b101), "5_2".to_string()),
            ],
            all
        );

        Ok(())
    }
}
