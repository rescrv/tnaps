use std::collections::btree_map::Entry;
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::{Bound, Deref};
use std::sync::{Mutex, MutexGuard};

use super::{ComponentChange, ComponentCollection, ComponentRef};
use crate::Entity;

//////////////////////////////////////////// Components ////////////////////////////////////////////

#[derive(Debug)]
struct Components<T: Debug> {
    components: Vec<Option<T>>,
    free: Vec<usize>,
}

impl<T: Debug> Default for Components<T> {
    fn default() -> Self {
        let components = vec![];
        let free = vec![];
        Self { components, free }
    }
}

//////////////////////////////////// InsertOptimizedComponentRef ///////////////////////////////////

/// The [ComponentRef] type for [InsertOptimizedComponentCollection].
pub struct InsertOptimizedComponentRef<'a, T: Debug> {
    this: MutexGuard<'a, Components<T>>,
    idx: usize,
}

impl<'a, T: Debug> InsertOptimizedComponentRef<'a, T> {
    fn new(this: MutexGuard<'a, Components<T>>, idx: usize) -> Self {
        assert!(idx < this.components.len());
        Self { this, idx }
    }
}

impl<'a, T: Debug> Debug for InsertOptimizedComponentRef<'a, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::result::Result<(), std::fmt::Error> {
        f.debug_struct("InsertOptimizedComponentRef<T>")
            .field("this", &self.this.components[self.idx])
            .finish()
    }
}

impl<'a, T: Debug> Deref for InsertOptimizedComponentRef<'a, T> {
    type Target = T;

    /// # Panics:
    ///
    /// This function panics if there was a previous call to unbind.
    fn deref(&self) -> &Self::Target {
        // SAFETY(rescrv):  Ensured by the caller.
        self.this.components[self.idx].as_ref().unwrap()
    }
}

impl<'a, T: Debug> ComponentRef<T> for InsertOptimizedComponentRef<'a, T> {
    fn unbind(&mut self) {
        if self.this.components[self.idx].is_some() {
            self.this.components[self.idx] = None;
            self.this.free.push(self.idx);
        }
    }

    /// # Panics:
    ///
    /// This function panics if there was a previous call to unbind.
    fn update<F: FnOnce(&mut T) -> U, U>(&mut self, f: F) -> U {
        f(self.this.components[self.idx].as_mut().unwrap())
    }

    fn change(self) -> ComponentChange<T> {
        ComponentChange::NoChange
    }
}

//////////////////////////////// InsertOptimizedComponentCollection ////////////////////////////////

/// An insert-optimized component collection.  This will allow for fast insertions and removals of
/// entities with the trade-off being that individual insertions and deletions will be more
/// efficient than individual insertions or deletions like in other collections, but only for small
/// update sizes.  For changes that touch more than a small number of components,
/// CopyOnWriteComponentCollection and MutableComponentCollection are preferred.
#[derive(Debug)]
pub struct InsertOptimizedComponentCollection<E: Entity, T: Debug> {
    entities: Mutex<BTreeMap<E, usize>>,
    components: Mutex<Components<T>>,
}

impl<E: Entity, T: Debug> InsertOptimizedComponentCollection<E, T> {
    /// Bind the provided component to the specified entity.
    pub fn insert(&self, entity: E, component: T) -> Option<T> {
        let mut entities = self.entities.lock().unwrap();
        let mut components = self.components.lock().unwrap();
        match entities.entry(entity) {
            Entry::Occupied(entry) => {
                let mut component = Some(component);
                std::mem::swap(&mut components.components[*entry.get()], &mut component);
                component
            }
            Entry::Vacant(entry) => {
                let index = if let Some(index) = components.free.pop() {
                    components.components[index] = Some(component);
                    index
                } else {
                    let index = components.components.len();
                    components.components.push(Some(component));
                    index
                };
                entry.insert(index);
                None
            }
        }
    }
}

impl<E: Entity, T: Debug> Default for InsertOptimizedComponentCollection<E, T> {
    fn default() -> Self {
        let entities = Mutex::new(BTreeMap::new());
        let components = Mutex::new(Components::default());
        Self {
            entities,
            components,
        }
    }
}

impl<E: Entity, T: Debug> ComponentCollection<E, T> for InsertOptimizedComponentCollection<E, T> {
    type Ref<'a> = InsertOptimizedComponentRef<'a, T> where Self: 'a, T: 'a;
    type Consumed = InsertOptimizedComponentCollectionIterator<E, T>;

    fn is_empty(&self) -> bool {
        self.entities.lock().unwrap().is_empty()
    }

    fn len(&self) -> usize {
        self.entities.lock().unwrap().len()
    }

    fn lower_bound(&self, lower_bound: E) -> Option<E> {
        let entities = self.entities.lock().unwrap();
        entities
            .range((Bound::Included(lower_bound), Bound::Unbounded))
            .next()
            .map(|x| *x.0)
    }

    fn get_ref(&self, entity: E) -> Option<Self::Ref<'_>> {
        let entities = self.entities.lock().unwrap();
        let components = self.components.lock().unwrap();
        if let Some(index) = entities.get(&entity) {
            if *index < components.components.len() {
                Some(InsertOptimizedComponentRef::new(components, *index))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn consume(self) -> Self::Consumed {
        let entities = self.entities.into_inner().unwrap().into_iter();
        let components = self.components.into_inner().unwrap().components;
        InsertOptimizedComponentCollectionIterator {
            entities,
            components,
        }
    }

    fn apply(&mut self, changes: Vec<(E, ComponentChange<T>)>) {
        for (e, change) in changes.into_iter() {
            if let Some(mut existing) = self.get_ref(e) {
                match change {
                    ComponentChange::NoChange => {}
                    ComponentChange::Unbind => {
                        existing.unbind();
                    }
                    ComponentChange::Value(t) => {
                        let t: T = t;
                        existing.update(|x| *x = t);
                    }
                };
            } else {
                match change {
                    ComponentChange::NoChange => {}
                    ComponentChange::Unbind => {}
                    ComponentChange::Value(t) => {
                        self.insert(e, t);
                    }
                };
            }
        }
    }
}

impl<E: Entity, T: Debug> FromIterator<(E, T)> for InsertOptimizedComponentCollection<E, T> {
    fn from_iter<I: IntoIterator<Item = (E, T)>>(iter: I) -> Self {
        let mut entities = BTreeMap::new();
        let mut components = vec![];
        iter.into_iter().for_each(|(e, t)| {
            entities.insert(e, components.len());
            components.push(Some(t));
        });
        let entities = Mutex::new(entities);
        let free = vec![];
        let components = Mutex::new(Components { components, free });
        Self {
            entities,
            components,
        }
    }
}

impl<E: Entity, T: Debug> FromIterator<(E, ComponentChange<T>)>
    for InsertOptimizedComponentCollection<E, T>
{
    fn from_iter<I: IntoIterator<Item = (E, ComponentChange<T>)>>(iter: I) -> Self {
        let mut entities = BTreeMap::new();
        let mut components = vec![];
        iter.into_iter().for_each(|(e, t)| {
            if let ComponentChange::Value(t) = t {
                entities.insert(e, components.len());
                components.push(Some(t));
            }
        });
        let entities = Mutex::new(entities);
        let free = vec![];
        let components = Mutex::new(Components { components, free });
        Self {
            entities,
            components,
        }
    }
}

//////////////////////////////////// ComponentCollectionIterator ///////////////////////////////////

/// An iterator over an [InsertOptimizedComponentCollection].
pub struct InsertOptimizedComponentCollectionIterator<E: Entity, T: Debug> {
    entities: std::collections::btree_map::IntoIter<E, usize>,
    components: Vec<Option<T>>,
}

impl<E: Entity, T: Debug> Iterator for InsertOptimizedComponentCollectionIterator<E, T> {
    type Item = (E, T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((e, idx)) = self.entities.next() {
                if let Some(t) = self.components[idx].take() {
                    return Some((e, t));
                }
            } else {
                return None;
            }
        }
    }
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
mod tests {
    use super::super::tests::{arb_entities, collection_properties};

    use super::InsertOptimizedComponentCollection;

    proptest::proptest! {
        #[test]
        fn insert_collection_properties(entities in arb_entities()) {
            collection_properties::<u128, usize, InsertOptimizedComponentCollection<u128, usize>>(entities);
        }
    }
}
