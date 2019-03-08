use gc_arena::{Collect, CollectionContext};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::default::Default;
use std::borrow::Borrow;
use std::mem;

fn do_hash<K: Hash+Eq>(k: &K) -> usize {
    let mut hasher = DefaultHasher::new();
    k.hash(&mut hasher);
    hasher.finish() as usize
}

#[derive(Clone, Debug)]
enum Bucket<K, V> {
    Empty,      // Empty
    Full(K, V), // An actual value
    Tombstone,  // Empty (but was once full)
}

impl<K, V> Default for Bucket<K, V> {
    fn default() -> Self { Bucket::Empty }
}

#[derive(Clone, Debug)]
pub(crate) struct HashMap<K, V> {
    buckets: Vec<Bucket<K, V>>,
    num_items: usize,
    capacity: usize,
}

const MIN_CAPACITY: usize = 8;

impl<K, V> Default for HashMap<K, V> {
    fn default() -> Self {
        let mut buckets = Vec::new();
        buckets.resize_with(MIN_CAPACITY, Default::default);
        let num_items = 0;
        let capacity = MIN_CAPACITY;
        HashMap {
            buckets, num_items, capacity,
        }
    }
}

pub struct HashMapIter<'a, K, V> {
    map: &'a HashMap<K, V>,
    idx: usize,
}

impl<'a, K, V> Iterator for HashMapIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        while self.idx < self.map.capacity {
            let idx = self.idx;
            self.idx += 1;
            match self.map.buckets[idx] {
                Bucket::Empty => (),
                Bucket::Tombstone => (),
                Bucket::Full(ref k, ref v) => { return Some((k, v)) }
            }
        }
        None
    }
}

impl<'a, K, V> IntoIterator for &'a HashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = HashMapIter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        HashMapIter {
            map: self,
            idx: 0,
        }
    }
}

unsafe impl<K, V> Collect for HashMap<K, V>
where
    K: Eq + Hash + Collect,
    V: Collect,
{
    #[inline]
    fn needs_trace() -> bool {
        K::needs_trace() || V::needs_trace()
    }

    #[inline]
    fn trace(&self, cc: CollectionContext) {
        for (k, v) in self {
            k.trace(cc);
            v.trace(cc);
        }
    }
}

impl<K: Eq + Hash, V> HashMap<K, V> {
    fn do_resize(&mut self, capacity: usize) {
        unimplemented!()
    }
}

impl<K: Eq + Hash, V> HashMap<K, V> {
    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&V>
    where K: Borrow<Q>,
          Q: Hash + Eq
    {
        unimplemented!()
    }

    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where K: Borrow<Q>,
          Q: Hash + Eq
    {
        unimplemented!()
    }

    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
    where K: Borrow<Q>,
          Q: Hash + Eq
    {
        unimplemented!()
    }

    pub fn len(&self) -> usize {
        self.num_items
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
    pub fn insert(&mut self, k: K, v: V) -> Option<V>
    {
        let hash = do_hash(&k);
        if self.num_items * 4 >= self.capacity * 3 {
            // three quarters full
            self.do_resize(self.capacity * 2);
        }
        let offset = hash % self.capacity;
        let mut idx = offset;
        loop {
            match self.buckets[idx] {
                Bucket::Empty |
                Bucket::Tombstone => {
                    // Found an empty bucket
                    self.buckets[idx] = Bucket::Full(k, v);
                    self.num_items += 1;
                    return None;
                }
                Bucket::Full(ref bk, ref mut bv) => {
                    if k == *bk {
                        return Some(mem::replace(bv, v));
                    }
                    // Else continue looking
                }
            }
            idx += 1;
            if idx >= self.capacity {
                idx = 0;
            }
            // If we get back to offset, then we must be full, but
            // we never allow that to happen.
            assert!(idx != offset);
        }
    }

    pub fn keys(&self) -> impl Iterator<Item=&K> {
        self.buckets.iter()
                    .filter_map(|bucket| {
                        if let Bucket::Full(k, _) = bucket {
                            Some(k)
                        } else {
                            None
                        }
                    })
    }
    pub fn retain<F>(&mut self, f: F) where
        F: FnMut(&K, &mut V) -> bool
    {
        unimplemented!()
    }
    pub fn reserve(&mut self, additional: usize)
    {
        // TBD
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn make_hashmap() {
        let _map = HashMap::<isize, isize>::default();
    }

    #[test]
    fn insert_one() {
        let mut map = HashMap::default();
        map.insert(1isize, 4isize);
    }

    #[test]
    fn iterate() {
        let mut map = HashMap::default();
        map.insert(1isize, 4isize);
        map.insert(2, 3);
        let mut items: Vec<_> = map.into_iter().collect();
        items.sort();
        assert_eq!(&items[..], &[(&1, &4), (&2, &3)]);
    }
}