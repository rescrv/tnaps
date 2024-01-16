use std::fmt::Debug;
use std::hash::Hash;

use crate::base64;

mod fast_map;
mod vec_map;

pub use fast_map::{FastEntityMap, FastEntityMapIntoIterator, FastEntityMapIterator};
pub use vec_map::VecEntityMap;

////////////////////////////////////////////// Entity //////////////////////////////////////////////

/// Entity is one part of the ECS triad.  It should be a Copy-able type that implements this trait.
/// Entities are restricted because they are used as pointers in all other code.  Implementations
/// of entity include u32, u64, and u128.
pub trait Entity: Copy + Default + Debug + Eq + Ord + Hash {
    /// Convert the entity to a display-able value.
    fn display(&self) -> String;
    /// Return the previous entity according to the total ordering of entities.
    fn decrement(self) -> Self;
    /// Return the next entity according to the total ordering of entities.
    fn increment(self) -> Self;
    /// Return the maximum entity possible.
    fn max_value() -> Self;
}

impl Entity for u32 {
    fn display(&self) -> String {
        let bytes = self.to_le_bytes();
        base64::encode(&bytes)
    }

    fn decrement(self) -> Self {
        self.wrapping_sub(1)
    }

    fn increment(self) -> Self {
        self.wrapping_add(1)
    }

    fn max_value() -> Self {
        Self::MAX
    }
}

impl Entity for u64 {
    fn display(&self) -> String {
        let bytes = self.to_le_bytes();
        base64::encode(&bytes)
    }

    fn decrement(self) -> Self {
        self.wrapping_sub(1)
    }

    fn increment(self) -> Self {
        self.wrapping_add(1)
    }

    fn max_value() -> Self {
        Self::MAX
    }
}

impl Entity for u128 {
    fn display(&self) -> String {
        let bytes = self.to_le_bytes();
        base64::encode(&bytes)
    }

    fn decrement(self) -> Self {
        self.wrapping_sub(1)
    }

    fn increment(self) -> Self {
        self.wrapping_add(1)
    }

    fn max_value() -> Self {
        Self::MAX
    }
}

///////////////////////////////////////////// EntityMap ////////////////////////////////////////////

/// EntityMap provides an interface for mapping entities to indices.  It also provides tools for
/// walking the total order of entities.
pub trait EntityMap<E: Entity>: Debug + IntoIterator<Item = E> + FromIterator<E> {
    /// The type returned by iter.
    type Iter<'a>: Iterator<Item = E> + 'a
    where
        Self: 'a;

    /// True if the entity map contains no entries.
    fn is_empty(&self) -> bool;
    /// The number of entities in the map.
    fn len(&self) -> usize;
    /// The entity at offset.
    ///
    /// # Panics
    ///
    /// If offset >= self.len().
    fn get(&self, offset: usize) -> E;
    /// The offset where this entity should sit.  This may point to an entity other than the one
    /// searched for.  See [Self::exact_offset_of] if this is undesirable.
    fn offset_of(&self, entity: E) -> usize;
    /// The exact offset of this entity.  Will return None if the entity is not in the map.
    fn exact_offset_of(&self, entity: E) -> Option<usize>;
    /// Return the first entity greater or equal to entity in the map.
    fn lower_bound(&self, entity: E) -> Option<E>;
    /// Iterate over all entities in the map.
    fn iter(&self) -> Self::Iter<'_>;
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::*;

    pub fn check_entity_map<E: Entity, EM: EntityMap<E>>(entities: Vec<E>, map: EM) {
        assert_eq!(entities.is_empty(), map.is_empty());
        assert_eq!(entities.len(), map.len());
        for (idx, (lhs, rhs)) in std::iter::zip(map.iter(), entities.iter()).enumerate() {
            assert_eq!(lhs, *rhs);
            assert_eq!(lhs, map.get(idx));
            assert_eq!(idx, map.offset_of(lhs));
            assert_eq!(Some(lhs), map.lower_bound(lhs));
            if idx > 0 && entities[idx - 1].increment() != entities[idx] {
                assert_eq!(idx, map.offset_of(lhs.decrement()));
                assert_eq!(Some(lhs), map.lower_bound(lhs.decrement()));
            }
        }
        for (expected, returned) in std::iter::zip(entities.iter(), map.into_iter()) {
            assert_eq!(*expected, returned);
        }
    }
}
