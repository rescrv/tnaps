use super::{Entity, EntityMap};

const FANOUT: usize = 31;
const IS_LEAF: u64 = 64;
const FLAG_MASK: u64 = 31;

/////////////////////////////////////////////// Node ///////////////////////////////////////////////

#[derive(Debug, Default)]
#[repr(C, align(64))]
struct Node<E: Entity> {
    flags: u64,
    offset: usize,
    entities: [E; FANOUT],
}

impl<E: Entity> Node<E> {
    fn leaf() -> Self {
        Self {
            flags: IS_LEAF,
            offset: 0,
            entities: [E::default(); FANOUT],
        }
    }

    fn internal(offset: usize) -> Self {
        Self {
            flags: 0,
            offset,
            entities: [E::default(); FANOUT],
        }
    }

    fn len(&self) -> usize {
        (self.flags & FLAG_MASK) as usize
    }

    fn lower_bound(&self, entity: E) -> usize {
        let sz = self.len();
        for (idx, e) in self.entities[..sz].iter().enumerate() {
            if *e >= entity {
                return idx;
            }
        }
        sz
    }
}

impl<E: Entity> From<Vec<E>> for Node<E> {
    fn from(ents: Vec<E>) -> Self {
        assert!(ents.len() <= FANOUT);
        assert!(!ents.iter().any(|e| *e == E::default()));
        let mut flags = IS_LEAF;
        flags += ents.len() as u64;
        let mut entities = [E::default(); FANOUT];
        entities[..ents.len()].copy_from_slice(&ents);
        Self {
            flags,
            offset: 0,
            entities,
        }
    }
}

/////////////////////////////////////// FastEntityMapIterator //////////////////////////////////////

/// FastEntityMapIterator is the iterator returned by [FastEntityMap::iter].
pub struct FastEntityMapIterator<'a, E: Entity> {
    nodes: &'a [Node<E>],
    index1: usize,
    index2: usize,
}

impl<'a, E: Entity> Iterator for FastEntityMapIterator<'a, E> {
    type Item = E;

    fn next(&mut self) -> Option<E> {
        if self.index1 >= self.nodes.len() || self.nodes[self.index1].flags & IS_LEAF == 0 {
            None
        } else {
            let entity = self.nodes[self.index1].entities[self.index2];
            self.index2 += 1;
            if self.index2 >= FANOUT {
                self.index2 = 0;
                self.index1 += 1;
            }
            if entity != E::default() {
                Some(entity)
            } else {
                None
            }
        }
    }
}

///////////////////////////////////// FastEntityMapIntoIterator ////////////////////////////////////

/// FastEntityMapIntoIterator is the iterator returned by [FastEntityMap::into_iter].
pub struct FastEntityMapIntoIterator<E: Entity> {
    nodes: Vec<Node<E>>,
    index1: usize,
    index2: usize,
}

impl<E: Entity> Iterator for FastEntityMapIntoIterator<E> {
    type Item = E;

    fn next(&mut self) -> Option<E> {
        if self.index1 >= self.nodes.len() || self.nodes[self.index1].flags & IS_LEAF == 0 {
            None
        } else {
            let entity = self.nodes[self.index1].entities[self.index2];
            self.index2 += 1;
            if self.index2 >= FANOUT {
                self.index2 = 0;
                self.index1 += 1;
            }
            if entity != E::default() {
                Some(entity)
            } else {
                None
            }
        }
    }
}

/////////////////////////////////////////// FastEntityMap //////////////////////////////////////////

/// FastEntityMap is a cache-friendlier version of an entity map, compared to vector or other
/// implementations.  In practice, FastEntityMap can be slower to construct, but provide faster
/// lookup times.
#[derive(Debug)]
pub struct FastEntityMap<E: Entity> {
    nodes: Vec<Node<E>>,
    size: usize,
}

impl<E: Entity> FastEntityMap<E> {
    fn offset_of_recursive(&self, entity: E, index: usize) -> usize {
        if self.nodes[index].flags & IS_LEAF != 0 {
            let offset = self.nodes[index].lower_bound(entity);
            index.saturating_mul(FANOUT).saturating_add(offset)
        } else {
            let offset = self.nodes[index].lower_bound(entity);
            self.offset_of_recursive(entity, self.nodes[index].offset + offset)
        }
    }

    fn lower_bound_recursive(&self, entity: E, divider: Option<E>, index: usize) -> Option<E> {
        if self.nodes[index].flags & IS_LEAF != 0 {
            let offset = self.nodes[index].lower_bound(entity);
            if offset < self.nodes[index].len() {
                Some(self.nodes[index].entities[offset])
            } else {
                divider
            }
        } else {
            let offset = self.nodes[index].lower_bound(entity);
            let divider = if offset < self.nodes[index].len() {
                Some(self.nodes[index].entities[offset])
            } else {
                divider
            };
            self.lower_bound_recursive(entity, divider, self.nodes[index].offset + offset)
        }
    }

    fn seal(size: usize, mut nodes: Vec<Node<E>>, start: usize, limit: usize) -> Self {
        if start + 1 >= limit {
            return Self { nodes, size };
        }
        nodes.reserve((limit - start + FANOUT - 1) / FANOUT);
        let new_start = nodes.len();
        let mut internal_index = 0;
        nodes.push(Node::<E>::internal(start));
        for child_index in start..limit {
            if child_index + 1 < limit {
                if internal_index >= FANOUT {
                    nodes.push(Node::<E>::internal(child_index));
                    internal_index = 0;
                }
                let last = nodes.len() - 1;
                assert_ne!(E::default(), nodes[child_index + 1].entities[0]);
                nodes[last].entities[internal_index] = nodes[child_index + 1].entities[0];
                nodes[last].flags += 1;
                internal_index += 1;
            }
        }
        let new_limit = nodes.len();
        Self::seal(size, nodes, new_start, new_limit)
    }
}

impl<E: Entity> EntityMap<E> for FastEntityMap<E> {
    type Iter<'a> = FastEntityMapIterator<'a, E> where Self: 'a;

    fn is_empty(&self) -> bool {
        self.nodes.is_empty() || self.nodes[self.nodes.len() - 1].len() == 0
    }

    fn len(&self) -> usize {
        self.size
    }

    fn get(&self, offset: usize) -> E {
        let index1 = offset / FANOUT;
        let index2 = offset % FANOUT;
        self.nodes[index1].entities[index2]
    }

    fn offset_of(&self, entity: E) -> usize {
        if self.nodes.is_empty() {
            0
        } else {
            self.offset_of_recursive(entity, self.nodes.len() - 1)
        }
    }

    fn exact_offset_of(&self, entity: E) -> Option<usize> {
        if self.nodes.is_empty() {
            None
        } else {
            let offset = self.offset_of_recursive(entity, self.nodes.len() - 1);
            if self.get(offset) == entity {
                Some(offset)
            } else {
                None
            }
        }
    }

    fn lower_bound(&self, entity: E) -> Option<E> {
        if self.nodes.is_empty() {
            None
        } else {
            self.lower_bound_recursive(entity, None, self.nodes.len() - 1)
        }
    }

    fn iter(&self) -> Self::Iter<'_> {
        FastEntityMapIterator {
            nodes: &self.nodes,
            index1: 0,
            index2: 0,
        }
    }
}

impl<E: Entity> IntoIterator for FastEntityMap<E> {
    type Item = E;
    type IntoIter = FastEntityMapIntoIterator<E>;

    fn into_iter(self) -> Self::IntoIter {
        FastEntityMapIntoIterator {
            nodes: self.nodes,
            index1: 0,
            index2: 0,
        }
    }
}

impl<E: Entity> FromIterator<E> for FastEntityMap<E> {
    fn from_iter<I: IntoIterator<Item = E>>(entities: I) -> Self {
        let mut nodes = vec![Node::<E>::leaf()];
        let mut index = 0;
        let prev_entity = E::default();
        let mut count = 0;
        for entity in entities {
            if index >= FANOUT {
                nodes.push(Node::<E>::leaf());
                index = 0;
            }
            assert!(prev_entity < entity);
            let last = nodes.len() - 1;
            nodes[last].entities[index] = entity;
            nodes[last].flags += 1;
            index += 1;
            count += 1;
        }
        let len = nodes.len();
        Self::seal(count, nodes, 0, len)
    }
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
mod tests {
    extern crate proptest;

    use proptest::strategy::Strategy;

    use super::super::tests::check_entity_map;
    use super::*;

    use crate::tests::{arb_entity, is_free_of_duplicates};

    proptest::prop_compose! {
        fn arb_entities_node()(mut entities in proptest::collection::vec(arb_entity(), 0..=FANOUT).prop_filter("dedupe", is_free_of_duplicates)) -> Vec<u128> {
            entities.sort();
            entities.dedup();
            entities
        }
    }

    proptest::prop_compose! {
        fn arb_entities_fast_map()(mut entities in proptest::collection::vec(arb_entity(), 0..(FANOUT * FANOUT * FANOUT)).prop_filter("dedupe", is_free_of_duplicates)) -> Vec<u128> {
            entities.sort();
            entities.dedup();
            entities
        }
    }

    proptest::proptest! {
        #[test]
        fn node(entities in arb_entities_node()) {
            let node = Node::from(entities.clone());
            assert_eq!(entities.len(), node.len());
            for (idx, e) in entities.iter().enumerate() {
                assert_eq!(idx, node.lower_bound(*e));
                if idx > 0 && entities[idx - 1].increment() != entities[idx] {
                    assert_eq!(idx, node.lower_bound(e.decrement()));
                }
            }
            assert_eq!(entities.len(), node.lower_bound(u128::MAX));
        }

        #[test]
        fn fast_map(entities in arb_entities_fast_map()) {
            let fast_map = FastEntityMap::from_iter(entities.clone().into_iter());
            check_entity_map(entities, fast_map);
        }
    }
}
