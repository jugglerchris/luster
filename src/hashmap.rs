use gc_arena::{Collect, CollectionContext};
use std::hash::{Hash};
use std::default::Default;
use std::borrow::Borrow;

#[derive(Clone, Debug)]
pub(crate) struct HashMap<K, V> {
    keys: Vec<K>,
    values: Vec<V>,
}

impl<K, V> Default for HashMap<K, V> {
    fn default() -> Self {
        HashMap {
            keys: Vec::new(),
            values: Vec::new(),
        }
    }
}

pub struct HashMapIter<'a, K, V> {
    map: &'a HashMap<K, V>,
}

impl<'a, K, V> Iterator for HashMapIter<'a, K, V> {
    type Item = (&'a K, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        unimplemented!()
    }
}

impl<'a, K, V> IntoIterator for &'a HashMap<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = HashMapIter<'a, K, V>;
    fn into_iter(self) -> Self::IntoIter {
        unimplemented!()
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

type Keys<'a, K> = std::slice::Iter<'a, K>;

impl<K, V> HashMap<K, V> {
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
        unimplemented!()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn capacity(&self) -> usize {
        unimplemented!()
    }
    pub fn insert(&mut self, k: K, v: V) -> Option<V>
    {
        unimplemented!()
    }

    pub fn keys(&self) -> Keys<K> {
        unimplemented!()
    }
    pub fn retain<F>(&mut self, f: F) where
        F: FnMut(&K, &mut V) -> bool
    {
        unimplemented!()
    }
    pub fn reserve(&mut self, additional: usize)
    {
        unimplemented!()
    }
}