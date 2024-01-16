use std::fmt::Debug;
use std::ops::Deref;

mod cow;
mod insert;
mod r#mut;

pub use cow::{CopyOnWriteComponentCollection, CopyOnWriteComponentRef};
pub use insert::{InsertOptimizedComponentCollection, InsertOptimizedComponentRef};
pub use r#mut::{MutableComponentCollection, MutableComponentRef};

use crate::partitioning::PartitioningScheme;
use crate::Entity;

//////////////////////////////////////// ComponentCollection ///////////////////////////////////////

/// ComponentCollection holds a set of `T` types in order sorted by entity.  `T` would be the
/// component type.
pub trait ComponentCollection<E: Entity, T: Debug>:
    Debug + Default + FromIterator<(E, T)> + FromIterator<(E, ComponentChange<T>)>
{
    /// A reference to a component.
    type Ref<'a>: ComponentRef<T>
    where
        Self: 'a,
        T: 'a;
    /// An iterator returned by the [Self::consume] call.
    type Consumed: Iterator<Item = (E, T)>;

    /// Is the collection empty?
    fn is_empty(&self) -> bool;
    /// How many elements are in the collection?
    fn len(&self) -> usize;

    /// What's the first entity greater-or-equal to the provided entity?
    fn lower_bound(&self, lower_bound: E) -> Option<E>;
    /// Get a reference to the component held for entity, if it exists.
    fn get_ref(&self, entity: E) -> Option<Self::Ref<'_>>;

    /// Consume the component collection.
    fn consume(self) -> Self::Consumed;

    /// Partition the collection according to the provided partitioning scheme.
    ///
    /// This function makes an arbitrary, but sorted, collection suitable for application to a
    /// partitioned collection.
    fn partition(self, partitioning: &dyn PartitioningScheme<E>) -> Vec<Option<Self>> {
        let mut consumed = self.consume();
        let mut consume_next = consumed.next();
        let mut partition = 0usize;
        let mut partitions = Vec::with_capacity(partitioning.len() + 1);
        let mut current_partition = vec![];
        while partition < partitioning.len() && consume_next.is_some() {
            let target = partitioning.partition(partition);
            // SAFETY(rescrv): Loop invariant asserts is_some().
            let c = consume_next.as_ref().unwrap();
            if c.0 < target {
                // SAFETY(rescrv): Loop invariant asserts is_some().
                current_partition.push(consume_next.unwrap());
                consume_next = consumed.next();
            } else {
                if !current_partition.is_empty() {
                    partitions.push(Some(Self::from_iter(current_partition)));
                } else {
                    partitions.push(None);
                }
                current_partition = vec![];
                partition += 1;
            }
        }
        while let Some(c) = consume_next {
            current_partition.push(c);
            consume_next = consumed.next();
        }
        partitions.push(Some(Self::from_iter(current_partition)));
        while partition < partitioning.len() {
            partitions.push(None);
            partition += 1;
        }
        assert_eq!(partitioning.len() + 1, partitions.len());
        partitions
    }

    /// Apply the changes to this collection.
    ///
    /// It is undefined behavior to pass a changes vector not sorted by entity value.
    fn apply(&mut self, changes: Vec<(E, ComponentChange<T>)>) {
        let this = std::mem::take(self);
        *self = apply_component_changes(this, changes.into_iter());
    }
}

/////////////////////////////////////////////// apply //////////////////////////////////////////////

pub(crate) fn apply_component_changes<
    E: Entity,
    T: Debug,
    C: ComponentCollection<E, T>,
    I: Iterator<Item = (E, ComponentChange<T>)>,
>(
    collection: C,
    mut changes: I,
) -> C {
    let mut changes_next = changes.next();
    if changes_next.is_none() {
        return collection;
    }
    let mut collected = Vec::with_capacity(collection.len());
    let mut collection = collection.consume();
    let mut collection_next = collection.next();
    while let (Some(c), Some(i)) = (collection_next.as_ref(), changes_next.as_ref()) {
        #[allow(clippy::comparison_chain)]
        if c.0 == i.0 {
            match &i.1 {
                ComponentChange::NoChange => {
                    // SAFETY(rescrv):  We see Some(c) above and haven't changed collection_next.
                    collected.push(collection_next.unwrap());
                }
                ComponentChange::Unbind => {
                    // pass
                }
                ComponentChange::Value(_) => {
                    // SAFETY(rescrv):  We see Some(i) above and haven't changed changes_next.
                    let (e, ComponentChange::Value(v)) = changes_next.unwrap() else {
                        unreachable!();
                    };
                    collected.push((e, v));
                }
            }
            collection_next = collection.next();
            changes_next = changes.next();
        } else if c.0 < i.0 {
            // SAFETY(rescrv):  We see Some(c) above and haven't changed collection_next.
            collected.push(collection_next.unwrap());
            collection_next = collection.next();
        } else {
            match &i.1 {
                ComponentChange::NoChange => {
                    // pass
                }
                ComponentChange::Unbind => {
                    // pass
                }
                ComponentChange::Value(_) => {
                    // SAFETY(rescrv):  We see Some(i) above and haven't changed changes_next.
                    let (e, ComponentChange::Value(v)) = changes_next.unwrap() else {
                        unreachable!();
                    };
                    collected.push((e, v));
                }
            }
            changes_next = changes.next();
        }
    }
    while collection_next.as_ref().is_some() {
        collected.push(collection_next.unwrap());
        collection_next = collection.next();
    }
    while let Some(i) = changes_next.as_ref() {
        match &i.1 {
            ComponentChange::NoChange => {
                // pass
            }
            ComponentChange::Unbind => {
                // pass
            }
            ComponentChange::Value(_) => {
                // SAFETY(rescrv):  We see Some(i) above and haven't changed changes_next.
                let (e, ComponentChange::Value(v)) = changes_next.unwrap() else {
                    unreachable!();
                };
                collected.push((e, v));
            }
        }
        changes_next = changes.next();
    }
    C::from_iter(collected)
}

////////////////////////////////////////// ComponentChange /////////////////////////////////////////

/// A change in the component.  This type is constructed by the ComponentRef, and should be passed
/// back to the collection via the apply call.
pub enum ComponentChange<T: Debug> {
    /// There was no change.  This is the default.
    NoChange,
    /// Unbind the component from the entity it's associated with, and free its memory.
    Unbind,
    /// Assing the value of T to the component when apply is called.
    Value(T),
}

impl<T: Debug> ComponentChange<T> {
    /// True if and only if this is a NoChange ComponentChange.
    pub fn is_no_change(&self) -> bool {
        matches!(self, Self::NoChange)
    }
}

/////////////////////////////////////////// ComponentRef ///////////////////////////////////////////

/// Reference a component.
pub trait ComponentRef<T: Debug>: Deref<Target = T> + Debug {
    /// Unbind the component, assuming the change generated by change is passed to the collection.
    /// In practice, this is done by taking the return value of running a system and passing the
    /// batch to apply.
    fn unbind(&mut self);
    /// Upudate the value and optionally return some state.
    fn update<F: FnOnce(&mut T) -> U, U>(&mut self, f: F) -> U;
    /// Consume this reference and make a [ComponentChange].  This is useful for saving a clone of
    /// T.
    fn change(self) -> ComponentChange<T>;
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
pub mod tests {
    extern crate proptest;

    use std::fmt::Debug;

    use proptest::strategy::Strategy;

    use super::ComponentCollection;

    use crate::tests::{arb_entity, is_free_of_duplicates};
    use crate::Entity;

    proptest::prop_compose! {
        pub fn arb_entities()(mut entities in proptest::collection::vec(arb_entity(), 0..=65536).prop_filter("dedupe", is_free_of_duplicates)) -> Vec<(u128, usize)> {
            entities.sort();
            entities.dedup();
            entities.into_iter().enumerate().map(|(i, x)| (x, i)).collect()
        }
    }

    pub fn collection_properties<E: Entity, T: Debug + Clone + Eq, C: ComponentCollection<E, T>>(
        collection: Vec<(E, T)>,
    ) {
        let components = C::from_iter(collection.clone());
        assert_eq!(collection.is_empty(), components.is_empty());
        assert_eq!(collection.len(), components.len());
        for (idx, (e, t)) in collection.iter().enumerate() {
            assert_eq!(Some(*e), components.lower_bound(*e));
            assert_eq!(*t, *components.get_ref(*e).unwrap());
            if idx > 0 && collection[idx - 1].0.increment() != collection[idx].0 {
                assert_eq!(Some(*e), components.lower_bound(e.decrement()));
                assert!(components.get_ref(e.decrement()).is_none());
            }
        }
        // TODO(partition);
        // TODO(apply);
        let consumed: Vec<(E, T)> = components.consume().collect();
        assert_eq!(collection, consumed);
    }
}
