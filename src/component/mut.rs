use std::fmt::Debug;
use std::ops::Deref;
use std::sync::{Mutex, MutexGuard};

use super::{ComponentChange, ComponentCollection, ComponentRef};
use crate::{Entity, EntityMap, VecEntityMap};

//////////////////////////////////////// MutableComponentRef ///////////////////////////////////////

/// The ComponentRef for MutableComponentCollection.
pub struct MutableComponentRef<'a, T: Debug> {
    unbound: bool,
    this: MutexGuard<'a, Vec<T>>,
    idx: usize,
}

impl<'a, T: Debug> MutableComponentRef<'a, T> {
    fn new(this: MutexGuard<'a, Vec<T>>, idx: usize) -> Self {
        let unbound = false;
        Self { unbound, this, idx }
    }
}

impl<'a, T: Debug> Debug for MutableComponentRef<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("MutableComponentRef<T>")
            .field("unbound", &self.unbound)
            .field("this", &self.this[self.idx])
            .finish()
    }
}

impl<'a, T: Debug> Deref for MutableComponentRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.this[self.idx]
    }
}

impl<'a, T: Debug> ComponentRef<T> for MutableComponentRef<'a, T> {
    fn unbind(&mut self) {
        self.unbound = true;
    }

    fn update<F: FnOnce(&mut T) -> U, U>(&mut self, f: F) -> U {
        f(&mut self.this[self.idx])
    }

    fn change(self) -> ComponentChange<T> {
        if self.unbound {
            ComponentChange::Unbind
        } else {
            ComponentChange::NoChange
        }
    }
}

//////////////////////////////////// MutableComponentCollection ////////////////////////////////////

/// A ComponentCollection that allows entities to be mutated in-place.  Useful for times when
/// most entities will be changed by a system.  The locking is at the granularity of the
/// collection, so it may be necessary to partition the collection for performance if multiple
/// systems operate on the collection in parallel.
///
/// If there's contention for the lock, consider making your type Send + Sync and using a
/// CopyOnWriteComponentCollection where you mutate the component from within a system.
#[derive(Debug)]
pub struct MutableComponentCollection<E: Entity, T: Debug> {
    entities: VecEntityMap<E>,
    components: Mutex<Vec<T>>,
}

impl<E: Entity, T: Debug> Default for MutableComponentCollection<E, T> {
    fn default() -> Self {
        let entities = VecEntityMap::from_iter(vec![]);
        let components = Mutex::new(Vec::new());
        Self {
            entities,
            components,
        }
    }
}

impl<E: Entity, T: Debug> ComponentCollection<E, T> for MutableComponentCollection<E, T> {
    type Ref<'a> = MutableComponentRef<'a, T> where Self: 'a, T: 'a;
    type Consumed = std::iter::Zip<std::vec::IntoIter<E>, std::vec::IntoIter<T>>;

    fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    fn len(&self) -> usize {
        self.entities.len()
    }

    fn lower_bound(&self, lower_bound: E) -> Option<E> {
        self.entities.lower_bound(lower_bound)
    }

    fn get_ref(&self, entity: E) -> Option<Self::Ref<'_>> {
        if let Some(offset) = self.entities.exact_offset_of(entity) {
            let components = self.components.lock().unwrap();
            Some(MutableComponentRef::new(components, offset))
        } else {
            None
        }
    }

    fn consume(self) -> Self::Consumed {
        let e = self.entities.into_iter();
        let t = self.components.into_inner().unwrap().into_iter();
        std::iter::zip(e, t)
    }
}

impl<E: Entity, T: Debug> FromIterator<(E, T)> for MutableComponentCollection<E, T> {
    fn from_iter<I: IntoIterator<Item = (E, T)>>(iter: I) -> Self {
        let mut entities = vec![];
        let mut components = vec![];
        iter.into_iter().for_each(|(e, t)| {
            entities.push(e);
            components.push(t);
        });
        let entities = VecEntityMap::from_iter(entities);
        let components = Mutex::new(components);
        Self {
            entities,
            components,
        }
    }
}

impl<E: Entity, T: Debug> FromIterator<(E, ComponentChange<T>)>
    for MutableComponentCollection<E, T>
{
    fn from_iter<I: IntoIterator<Item = (E, ComponentChange<T>)>>(iter: I) -> Self {
        let mut entities = vec![];
        let mut components = vec![];
        iter.into_iter().for_each(|(e, t)| {
            if let ComponentChange::Value(t) = t {
                entities.push(e);
                components.push(t);
            }
        });
        let entities = VecEntityMap::from_iter(entities);
        let components = Mutex::new(components);
        Self {
            entities,
            components,
        }
    }
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::super::tests::{arb_entities, collection_properties};

    use super::MutableComponentCollection;

    proptest::proptest! {
        #[test]
        fn mut_collection_properties(entities in arb_entities()) {
            collection_properties::<u128, usize, MutableComponentCollection<u128, usize>>(entities);
        }
    }
}
