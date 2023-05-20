use indexmap::IndexMap;

pub struct PriorityQueue<K, T> {
    inner: IndexMap<K, T>,
}

impl<K, T> PriorityQueue<K, T> {
    pub fn new() -> Self {
        Self {
            inner: IndexMap::new(),
        }
    }

    pub fn peek(&self) -> Option<(&K, &T)> {
        self.inner.first()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn iter(&self) -> indexmap::map::Values<'_, K, T> {
        self.inner.values()
    }
}

impl<K, T> PriorityQueue<K, T>
where
    K: core::hash::Hash + Eq,
    T: Ord,
{
    pub fn push(&mut self, key: K, val: T) -> Option<T> {
        let (index, old_val) = self.inner.insert_full(key, val);
        self.bubble_up(index);
        if old_val.is_some() {
            self.bubble_down(index);
        }
        old_val
    }

    pub fn modify<F, Q>(&mut self, key: &Q, modifier: F) -> bool
    where
        F: FnOnce(&mut T),
        Q: std::borrow::Borrow<K>,
    {
        if let Some((i, _, v)) = self.inner.get_full_mut(key.borrow()) {
            modifier(v);
            self.bubble_up(i);
            self.bubble_down(i);
            true
        } else {
            false
        }
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<(K, T)>
    where
        Q: std::borrow::Borrow<K>,
    {
        if let Some((i, k, v)) = self.inner.swap_remove_full(key.borrow()) {
            if !self.inner.is_empty() {
                self.bubble_up(i);
                self.bubble_down(i);
            }
            Some((k, v))
        } else {
            None
        }
    }

    pub fn retain<F>(&mut self, mut modifier: F)
    where
        F: FnMut(&mut T) -> bool,
    {
        self.inner.retain(|_, v| modifier(v));
        self.bubble_all();
    }
}

impl<K, T> PriorityQueue<K, T>
where
    T: Ord,
{
    pub fn pop(&mut self) -> Option<(K, T)> {
        let root = self.inner.swap_remove_index(0);
        if !self.inner.is_empty() {
            self.bubble_down(0);
        }
        root
    }

    fn bubble_up(&mut self, i: usize) {
        match parent_index(i) {
            Some(p) if self.inner[i] < self.inner[p] => {
                self.inner.swap_indices(p, i);
                self.bubble_up(p);
            }
            _ => (),
        }
    }

    fn bubble_down(&mut self, i: usize) {
        let left = left_child_index(i).expect("will probably not be that big");
        let right = right_child_index(i).expect("will probably not be that big");
        let parent = &self.inner[i];

        let swap_with = match (self.inner.get_index(left), self.inner.get_index(right)) {
            (Some((_, l)), Some((_, r))) if l <= r && l < parent => Some(left),
            (Some((_, l)), Some((_, r))) if l > r && r < parent => Some(right),
            (Some((_, l)), None) if l < parent => Some(left),
            _ => None,
        };

        if let Some(swap) = swap_with {
            self.inner.swap_indices(i, swap);
            self.bubble_down(swap);
        }
    }

    fn bubble_all(&mut self) {
        if self.inner.is_empty() {
            return;
        }
        let last = self.inner.len() - 1;
        let Some(last_node_with_children) = parent_index(last) else {
            return;
        };
        for i in (0..=last_node_with_children).rev() {
            self.bubble_down(i);
        }
    }
}

fn parent_index(i: usize) -> Option<usize> {
    (i > 0).then(|| (i - 1) / 2)
}

fn left_child_index(i: usize) -> Option<usize> {
    i.checked_mul(2).and_then(|i| i.checked_add(1))
}

fn right_child_index(i: usize) -> Option<usize> {
    left_child_index(i).and_then(|i| i.checked_add(1))
}

#[cfg(test)]
mod test {
    use super::*;

    impl<K, T> PriorityQueue<K, T>
    where
        T: Ord,
    {
        fn pop_all(&mut self) -> Vec<T> {
            let mut v = Vec::new();
            while let Some((_, x)) = self.pop() {
                v.push(x);
            }
            v
        }

        fn invariant_holds(&self) -> bool {
            for i in 0..self.inner.len() {
                if let Some(parent) = parent_index(i) {
                    if self.inner[parent] > self.inner[i] {
                        return false;
                    }
                }
            }
            true
        }
    }

    impl<T> PriorityQueue<usize, T>
    where
        T: Ord,
    {
        fn push_all<I>(&mut self, iter: I)
        where
            I: IntoIterator<Item = T>,
        {
            for (i, t) in iter.into_iter().enumerate() {
                self.push(i, t);
            }
        }
    }

    impl<K, T> PriorityQueue<K, T>
    where
        T: Clone + Ord,
    {
        fn pop_all_cloned(&self) -> Vec<T> {
            let mut elements: Vec<T> = self.iter().cloned().collect();
            elements.sort();
            elements
        }
    }

    #[test]
    fn basic() {
        let mut que = PriorityQueue::<i32, i32>::new();
        assert!(que.is_empty());

        assert_eq!(None, que.remove(&1));
        assert!(que.is_empty());

        assert_eq!(None, que.pop());
        assert!(que.is_empty());

        assert_eq!(None, que.push(1, 1));
        assert_eq!(Some((&1, &1)), que.peek());
        assert_eq!(Some((1, 1)), que.pop());
        assert!(que.is_empty());

        assert_eq!(None, que.push(1, 1));
        assert_eq!(Some((1, 1)), que.remove(&1));
        assert!(que.is_empty());
        assert_eq!(None, que.peek());
        assert_eq!(None, que.pop());
    }

    #[test]
    fn duplicates() {
        let mut que = PriorityQueue::<i32, i32>::new();
        assert_eq!(None, que.push(1, 1));
        assert_eq!(Some(1), que.push(1, 2));
        assert_eq!(Some((1, 2)), que.pop());
        assert_eq!(None, que.push(1, 3));
    }

    #[test]
    fn sort() {
        let mut que = PriorityQueue::<usize, i32>::new();
        que.push_all([4, 1, 8, 6, 4, 6, 6, 1]);
        assert!(que.invariant_holds());
        assert_eq!(vec![1, 1, 4, 4, 6, 6, 6, 8], que.pop_all());
    }

    #[test]
    fn retain() {
        let mut que = PriorityQueue::<usize, i32>::new();
        assert!(que.invariant_holds());
        que.push_all([4, 1, 8, 6, 4, 6, 6, 1]);
        assert!(que.invariant_holds());
        que.retain(|x| {
            *x = 5 - *x;
            *x >= 0
        });
        assert!(que.invariant_holds());
        assert_eq!(vec![1, 1, 4, 4], que.pop_all());
        assert!(que.invariant_holds());
    }

    #[test]
    fn modify() {
        let mut que = PriorityQueue::<usize, i32>::new();
        que.push_all([9, 8, 7, 6, 5, 4, 3, 2, 1]);

        assert!(que.invariant_holds());
        let elements = que.pop_all_cloned();
        assert_eq!(vec![1, 2, 3, 4, 5, 6, 7, 8, 9i32], elements);

        que.modify(&1, |_| ());
        assert!(que.invariant_holds());
        let elements = que.pop_all_cloned();
        assert_eq!(vec![1, 2, 3, 4, 5, 6, 7, 8, 9i32], elements);

        que.modify(&1, |eight| *eight = 0);
        assert!(que.invariant_holds());
        let elements = que.pop_all_cloned();
        assert_eq!(vec![0, 1, 2, 3, 4, 5, 6, 7, 9i32], elements);

        que.modify(&1, |zero| *zero = 8);
        assert!(que.invariant_holds());
        let elements = que.pop_all_cloned();
        assert_eq!(vec![1, 2, 3, 4, 5, 6, 7, 8, 9i32], elements);
    }
}
