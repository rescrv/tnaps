use super::{Entity, EntityMap};

/////////////////////////////////////////// VecEntityMap ///////////////////////////////////////////

/// VecEntityMap uses binary search over a vector of entities.
#[derive(Debug)]
pub struct VecEntityMap<E: Entity> {
    entities: Vec<E>,
}

impl<E: Entity> EntityMap<E> for VecEntityMap<E> {
    type Iter<'a> = std::iter::Copied<std::slice::Iter<'a, E>> where Self: 'a;

    fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    fn len(&self) -> usize {
        self.entities.len()
    }

    fn get(&self, offset: usize) -> E {
        self.entities[offset]
    }

    fn offset_of(&self, entity: E) -> usize {
        self.entities.partition_point(|e| *e < entity)
    }

    fn exact_offset_of(&self, entity: E) -> Option<usize> {
        let offset = self.entities.partition_point(|e| *e < entity);
        if self.entities[offset] == entity {
            Some(offset)
        } else {
            None
        }
    }

    fn lower_bound(&self, entity: E) -> Option<E> {
        let offset = self.offset_of(entity);
        if offset < self.entities.len() {
            Some(self.entities[offset])
        } else {
            None
        }
    }

    fn iter(&self) -> Self::Iter<'_> {
        self.entities.iter().copied()
    }
}

impl<E: Entity> IntoIterator for VecEntityMap<E> {
    type Item = E;
    type IntoIter = std::vec::IntoIter<E>;

    fn into_iter(self) -> Self::IntoIter {
        self.entities.into_iter()
    }
}

impl<E: Entity> FromIterator<E> for VecEntityMap<E> {
    fn from_iter<I: IntoIterator<Item = E>>(entities: I) -> Self {
        let entities: Vec<E> = entities.into_iter().collect();
        Self { entities }
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
        fn arb_entities_vec_map()(mut entities in proptest::collection::vec(arb_entity(), 0..(15 * 15 * 15)).prop_filter("dedupe", is_free_of_duplicates)) -> Vec<u128> {
            entities.sort();
            entities.dedup();
            entities
        }
    }

    proptest::proptest! {
        #[test]
        fn vec_map(entities in arb_entities_vec_map()) {
            let vec_map = VecEntityMap::from_iter(entities.clone().into_iter());
            check_entity_map(entities, vec_map);
        }
    }
}
