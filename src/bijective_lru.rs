use std::num::NonZeroUsize;

use lru::LruCache;

// Bijection between (original source, destination) <-> (gateway port, destination)

pub struct BijectiveLru<K, V> {
    key_value: LruCache<K, V>,
    value_key: LruCache<V, K>,
}

impl<K: std::hash::Hash + Eq + Clone, V: std::hash::Hash + Eq + Clone> BijectiveLru<K, V> {
    pub fn new(cap: usize) -> Self {
        let cap = NonZeroUsize::new(cap).unwrap();
        let key_value = LruCache::new(cap);
        let value_key = LruCache::new(cap);
        Self {
            key_value,
            value_key,
        }
    }

    pub fn get_value(&mut self, key: &K) -> Option<&V> {
        self.key_value.get(key)
    }

    pub fn get_key(&mut self, value: &V) -> Option<&K> {
        self.value_key.get(value)
    }

    pub fn push(&mut self, key: K, value: V) -> Option<(K, V)> {
        self.value_key.push(value.clone(), key.clone());
        self.key_value.push(key, value)
    }
}

#[cfg(test)]
mod tests {
    use crate::bijective_lru::BijectiveLru;

    // tests that things don't get pushed out when table is not full
    #[test]
    fn test_1() {
        let mut table = BijectiveLru::new(1);
        table.push(1, 2);
        assert_eq!(table.get_value(&1).unwrap(), &2);
        assert_eq!(table.get_key(&2).unwrap(), &1);
    }

    #[test]
    // tests that the oldest entries get pushed out when table is full
    fn test2() {
        let mut table = BijectiveLru::new(2);
        table.push(1, 2);
        table.push(3, 4);
        assert_eq!(table.get_value(&1), Some(&2));
        table.push(5, 6);

        assert_eq!(table.get_value(&3), None);
        assert_eq!(table.get_key(&4), Some(&3));
    }

    #[test]
    fn test3() {
        let mut table = BijectiveLru::new(2);
        table.push(1, 2);
        table.push(3, 4);
        table.push(5, 6);

        assert_eq!(table.get_value(&1), None);

        assert_eq!(table.get_value(&5).unwrap(), &6);
        assert_eq!(table.get_value(&3).unwrap(), &4);
    }
}
