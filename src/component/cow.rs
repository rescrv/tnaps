use std::fmt::Debug;
use std::ops::Deref;

use super::{ComponentChange, ComponentCollection, ComponentRef};
use crate::{Entity, EntityMap, VecEntityMap};

////////////////////////////////////// CopyOnWriteComponentRef /////////////////////////////////////

/// Component ref for the [CopyOnWriteComponentCollection]
pub struct CopyOnWriteComponentRef<'a, T: Debug> {
    unbound: bool,
    this: &'a T,
    out: Option<T>,
}

impl<'a, T: Debug> CopyOnWriteComponentRef<'a, T> {
    fn new(this: &'a T) -> Self {
        let unbound = false;
        let out = None;
        Self { unbound, this, out }
    }
}

impl<'a, T: Debug> Debug for CopyOnWriteComponentRef<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("CopyOnWriteComponentRef<T>")
            .field("unbound", &self.unbound)
            .field("this", &self.this)
            .field("out", &self.out)
            .finish()
    }
}

impl<'a, T: Debug> Deref for CopyOnWriteComponentRef<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.out.as_ref().unwrap_or(self.this)
    }
}

impl<'a, T: Debug + Clone> ComponentRef<T> for CopyOnWriteComponentRef<'a, T> {
    fn unbind(&mut self) {
        self.unbound = true;
    }

    fn update<F: FnOnce(&mut T) -> U, U>(&mut self, f: F) -> U {
        if self.out.is_none() {
            self.out = Some(self.this.clone());
        }
        f(self.out.as_mut().unwrap())
    }

    fn change(self) -> ComponentChange<T> {
        if self.unbound {
            ComponentChange::Unbind
        } else if let Some(value) = self.out {
            ComponentChange::Value(value)
        } else {
            ComponentChange::NoChange
        }
    }
}

////////////////////////////////// CopyOnWriteComponentCollection //////////////////////////////////

/// CopyOnWrite component collection maintains a set of components in order, sorted by entity.  Any
/// calls to update or unbind will return a [ComponentChange] that won't take effect until it is
/// subsequently passed to `apply`.
#[derive(Debug)]
pub struct CopyOnWriteComponentCollection<E: Entity, T: Debug> {
    entities: VecEntityMap<E>,
    components: Vec<T>,
}

impl<E: Entity, T: Debug> Default for CopyOnWriteComponentCollection<E, T> {
    fn default() -> Self {
        let entities = VecEntityMap::from_iter(vec![]);
        let components = Vec::new();
        Self {
            entities,
            components,
        }
    }
}

impl<E: Entity, T: Debug + Clone> ComponentCollection<E, T>
    for CopyOnWriteComponentCollection<E, T>
{
    type Ref<'a> = CopyOnWriteComponentRef<'a, T> where Self: 'a, T: 'a;
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
        self.entities
            .exact_offset_of(entity)
            .map(|offset| CopyOnWriteComponentRef::new(&self.components[offset]))
    }

    fn consume(self) -> Self::Consumed {
        std::iter::zip(self.entities, self.components)
    }
}

impl<E: Entity, T: Debug> FromIterator<(E, T)> for CopyOnWriteComponentCollection<E, T> {
    fn from_iter<I: IntoIterator<Item = (E, T)>>(iter: I) -> Self {
        let mut entities = vec![];
        let mut components = vec![];
        iter.into_iter().for_each(|(e, t)| {
            entities.push(e);
            components.push(t);
        });
        let entities = VecEntityMap::from_iter(entities);
        Self {
            entities,
            components,
        }
    }
}

impl<E: Entity, T: Debug> FromIterator<(E, ComponentChange<T>)>
    for CopyOnWriteComponentCollection<E, T>
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

    use super::CopyOnWriteComponentCollection;

    proptest::proptest! {
        #[test]
        fn cow_collection_properties(entities in arb_entities()) {
            collection_properties::<u128, usize, CopyOnWriteComponentCollection<u128, usize>>(entities);
        }
    }
}
