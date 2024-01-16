use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::component::{apply_component_changes, ComponentChange, ComponentCollection};
use crate::{Entity, ThreadPool, WorkUnit};

//////////////////////////////////////// PartitioningScheme ////////////////////////////////////////

/// PartitioningScheme divides a totally-ordered entity-space into partitions.
pub trait PartitioningScheme<E: Entity>: Debug {
    /// Whether the partitioning scheme has dividers.
    fn is_empty(&self) -> bool;
    /// The number of partition dividers.  There will be one more partition than this number.
    fn len(&self) -> usize;
    /// Return the entity that serves as an upper-bound on partition.
    fn partition(&self, partition: usize) -> E;
    /// Compute the first partition in which the entity could reside.
    fn lower_bound(&self, entity: E) -> usize;
}

/////////////////////////////////////// NopPartitioningScheme //////////////////////////////////////

/// NopPartitioningScheme provides no partitioning whatsoever.
#[derive(Debug)]
pub struct NopPartitioningScheme;

impl<E: Entity> PartitioningScheme<E> for NopPartitioningScheme {
    fn is_empty(&self) -> bool {
        true
    }

    fn len(&self) -> usize {
        0
    }

    fn partition(&self, _: usize) -> E {
        // SAFETY(rescrv):  It is only valid to call partition on values < len().
        panic!("calling partition on a NopPartitioningScheme");
    }

    fn lower_bound(&self, _: E) -> usize {
        0
    }
}

/////////////////////////////////////// VecPartitioningScheme //////////////////////////////////////

/// Use a vector for partitioning.  Binary search will be used to find the appropriate partition.
#[derive(Debug)]
pub struct VecPartitioningScheme<E: Entity> {
    entities: Vec<E>,
}

impl<E: Entity> From<Vec<E>> for VecPartitioningScheme<E> {
    fn from(entities: Vec<E>) -> Self {
        Self { entities }
    }
}

impl<E: Entity> PartitioningScheme<E> for VecPartitioningScheme<E> {
    fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }

    fn len(&self) -> usize {
        self.entities.len()
    }

    fn partition(&self, partition: usize) -> E {
        assert!(partition < self.entities.len());
        self.entities[partition]
    }

    fn lower_bound(&self, entity: E) -> usize {
        self.entities.partition_point(|x| *x < entity)
    }
}

//////////////////////////////////////////// Partitioned ///////////////////////////////////////////

/// Partitioned wraps another collection type and partitions it according to the partitioning
/// scheme provided.
pub struct Partitioned<E: Entity, T: Debug, C: ComponentCollection<E, T>> {
    partitioning: Arc<dyn PartitioningScheme<E>>,
    partitions: Vec<Option<Arc<C>>>,
    _phantom_t: std::marker::PhantomData<T>,
}

impl<E: Entity, T: Debug, C: ComponentCollection<E, T>> Partitioned<E, T, C> {
    /// Create a new partitioned collection from the partitioning and partitions provided.
    pub fn from(partitioning: &Arc<dyn PartitioningScheme<E>>, partitions: Vec<Option<C>>) -> Self {
        let partitioning = Arc::clone(partitioning);
        let partitions = partitions.into_iter().map(|x| x.map(Arc::new)).collect();
        let _phantom_t = std::marker::PhantomData;
        Self {
            partitioning,
            partitions,
            _phantom_t,
        }
    }

    /// The partitioning scheme in use by this partitioned collection.
    pub fn partitioning_scheme(&self) -> &Arc<dyn PartitioningScheme<E>> {
        &self.partitioning
    }

    /// Return the N'th partition.
    pub fn get_partition_by_index(&self, partition: usize) -> Option<Arc<C>> {
        if partition < self.partitions.len() {
            self.partitions[partition].as_ref().map(Arc::clone)
        } else {
            None
        }
    }

    /// Apply the pre-partitioned changes to the collection.
    ///
    /// Behavior is undefined if the changes are not partitioned according to the partitioning of
    /// this partitioned collection.
    pub fn apply(&mut self, partitioned_changes: Vec<Vec<(E, ComponentChange<T>)>>) {
        self.apply_inner(partitioned_changes, |col, chan| {
            apply_component_changes(col, chan.into_iter())
        })
    }

    fn apply_inner<F: FnMut(C, Vec<(E, ComponentChange<T>)>) -> C + Clone>(
        &mut self,
        partitioned_changes: Vec<Vec<(E, ComponentChange<T>)>>,
        f: F,
    ) {
        assert_eq!(self.partitions.len(), partitioned_changes.len());
        let partitions = std::mem::take(&mut self.partitions);
        for (partition, changes) in
            std::iter::zip(partitions.into_iter(), partitioned_changes.into_iter())
        {
            self.partitions
                .push(Self::apply_partition(partition, changes, f.clone()));
        }
    }

    fn apply_partition<F: FnMut(C, Vec<(E, ComponentChange<T>)>) -> C>(
        partition: Option<Arc<C>>,
        changes: Vec<(E, ComponentChange<T>)>,
        mut f: F,
    ) -> Option<Arc<C>> {
        if let Some(ptr) = partition {
            if let Some(partition) = Arc::into_inner(ptr) {
                let partition = f(partition, changes);
                if !partition.is_empty() {
                    Some(Arc::new(partition))
                } else {
                    None
                }
            } else {
                panic!("`apply` method called while someone holds a reference to a partition");
            }
        } else {
            let partition: C = C::from_iter(changes);
            if !partition.is_empty() {
                Some(Arc::new(partition))
            } else {
                None
            }
        }
    }
}

impl<E: Entity + Send + Sync + 'static, T: Debug + Send + Sync + 'static, C: ComponentCollection<E, T> + Send + Sync + 'static> Partitioned<E, T, C> {
    /// Use `thread_pool` to apply the pre-partitioned changes in parallel.
    ///
    /// Behavior is undefined if the changes are not partitioned according to the partitioning of
    /// this partitioned collection.
    pub fn apply_parallel(&mut self, thread_pool: &ThreadPool, partitioned_changes: Vec<Vec<(E, ComponentChange<T>)>>) -> impl FnOnce() + '_ {
        assert_eq!(self.partitions.len(), partitioned_changes.len());
        let partitions = std::mem::take(&mut self.partitions);
        struct AggregatePartitions<E: Entity + Send, T: Debug + Send, C: ComponentCollection<E, T> + Send> {
            partitions: Mutex<Vec<Option<Arc<C>>>>,
            done: AtomicUsize,
            wait: Condvar,
            _phantom_e: std::marker::PhantomData<E>,
            _phantom_t: std::marker::PhantomData<T>,
        }
        impl<E: Entity + Send, T: Debug + Send, C: ComponentCollection<E, T> + Send> AggregatePartitions<E, T, C> {
            fn new(num_partitions: usize) -> Self {
                let mut partitions = Vec::with_capacity(num_partitions);
                for _ in 0..num_partitions {
                    partitions.push(None);
                }
                let partitions = Mutex::new(partitions);
                let done = AtomicUsize::new(0);
                let wait = Condvar::new();
                Self {
                    partitions,
                    done,
                    wait,
                    _phantom_e: std::marker::PhantomData,
                    _phantom_t: std::marker::PhantomData,
                }
            }

            fn done(&self, partition: usize, results: Option<Arc<C>>) {
                let len = {
                    let mut partitions = self.partitions.lock().unwrap();
                    if partitions[partition].is_none() {
                        // SAFETY(rescrv):  We need this Some(_) assignment to be the only
                        // one, and it must be 1:1 with the fetch_add.
                        partitions[partition] = results;
                        self.done.fetch_add(1, Ordering::Relaxed);
                    }
                    partitions.len()
                };
                if len == self.done.load(Ordering::Relaxed) {
                    self.wait.notify_all();
                }
            }

            fn wait(&self) -> Vec<Option<Arc<C>>> {
                let mut partitions = self.partitions.lock().unwrap();
                while self.done.load(Ordering::Relaxed) < partitions.len() {
                    partitions = self.wait.wait(partitions).unwrap();
                }
                let mut returned = vec![];
                std::mem::swap(&mut *partitions, &mut returned);
                returned
            }
        }
        let agg = Arc::new(AggregatePartitions::new(partitions.len()));
        for (idx, (partition, changes)) in
            std::iter::zip(partitions.into_iter(), partitioned_changes.into_iter()).enumerate()
        {
            let agg = Arc::clone(&agg);
            let work_unit: Box<WorkUnit> = Box::new(move || {
                let results = Self::apply_partition(partition, changes, |col, chan|apply_component_changes(col, chan.into_iter()));
                agg.done(idx, results);
            });
            thread_pool.enqueue(work_unit);
        }
        move || {
            self.partitions = agg.wait();
        }
    }
}

impl<E: Entity, T: Debug, C: ComponentCollection<E, T>> ComponentCollection<E, T> for Partitioned<E, T, C> {
    type Ref<'a> = C::Ref<'a> where Self: 'a;
    type Consumed = std::iter::Flatten<std::vec::IntoIter<<C as ComponentCollection<E, T>>::Consumed>>;

    fn is_empty(&self) -> bool {
        self.partitions.iter().all(|p| p.as_ref().map(|x| x.is_empty()).unwrap_or(true))
    }

    fn len(&self) -> usize {
        self.partitions.iter().map(|p| p.as_ref().map(|c| c.len()).unwrap_or(0usize)).fold(0usize, usize::saturating_add)
    }

    fn lower_bound(&self, lower_bound: E) -> Option<E> {
        let mut partition = self.partitioning.lower_bound(lower_bound);
        while partition < self.partitions.len() {
            let Some(p) = self.partitions[partition].as_ref() else {
                partition += 1;
                continue;
            };
            if let Some(lower_bound) = p.lower_bound(lower_bound) {
                return Some(lower_bound)
            }
            partition += 1;
        }
        None
    }

    fn get_ref(&self, entity: E) -> Option<Self::Ref<'_>> {
        let partition = self.partitioning.lower_bound(entity);
        self.partitions[partition].as_ref().and_then(|p| p.get_ref(entity))
    }

    fn consume(self) -> Self::Consumed {
        let mut partitions = Vec::with_capacity(self.partitions.len());
        for partition in self.partitions.into_iter().flatten() {
            if let Some(partition) = Arc::into_inner(partition) {
                partitions.push(partition.consume())
            } else {
                panic!("`consume` method called while someone holds a reference to a partition");
            }
        }
        partitions.into_iter().flatten()
    }
}

impl<E: Entity, T: Debug, C: ComponentCollection<E, T>> Default for Partitioned<E, T, C> {
    fn default() -> Self {
        let partitioning = Arc::new(NopPartitioningScheme);
        let partitions = vec![None];
        let _phantom_t = std::marker::PhantomData;
        Self {
            partitioning,
            partitions,
            _phantom_t,
        }
    }
}

impl<E: Entity, T: Debug, C: ComponentCollection<E, T>> Debug for Partitioned<E, T, C> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.debug_struct("Partitioned<E, X>")
            .field("partitioning", &self.partitioning)
            .field("partitions", &self.partitions)
            .finish()
    }
}

impl<E: Entity, T: Debug, C: ComponentCollection<E, T>> FromIterator<(E, T)> for Partitioned<E, T, C> {
    fn from_iter<I: IntoIterator<Item = (E, T)>>(iter: I) -> Self {
        let components = C::from_iter(iter);
        let partitioning = Arc::new(NopPartitioningScheme);
        let partitions = vec![Some(Arc::new(components))];
        let _phantom_t = std::marker::PhantomData;
        Self {
            partitioning,
            partitions,
            _phantom_t,
        }
    }
}

impl<E: Entity, T: Debug, C: ComponentCollection<E, T>> FromIterator<(E, ComponentChange<T>)> for Partitioned<E, T, C> {
    fn from_iter<I: IntoIterator<Item = (E, ComponentChange<T>)>>(iter: I) -> Self {
        let components = C::from_iter(iter);
        let partitioning = Arc::new(NopPartitioningScheme);
        let partitions = vec![Some(Arc::new(components))];
        let _phantom_t = std::marker::PhantomData;
        Self {
            partitioning,
            partitions,
            _phantom_t,
        }
    }
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
mod tests {
    extern crate proptest;

    use std::fmt::Debug;
    use std::sync::Arc;

    use proptest::strategy::Strategy;

    use crate::tests::{arb_entity, is_free_of_duplicates};
    use crate::{ComponentCollection, Entity, MutableComponentCollection};
    use crate::component::tests::collection_properties;

    use super::{NopPartitioningScheme, PartitioningScheme, Partitioned, VecPartitioningScheme};

    proptest::prop_compose! {
        pub fn arb_entities()(mut entities in proptest::collection::vec(arb_entity(), 0..=65536).prop_filter("dedupe", is_free_of_duplicates)) -> Vec<(u128, usize)> {
            entities.sort();
            entities.dedup();
            entities.into_iter().enumerate().map(|(i, x)| (x, i)).collect()
        }
    }

    proptest::prop_compose! {
        pub fn arb_partitions()(mut entities in proptest::collection::vec(arb_entity(), 0..=256).prop_filter("dedupe", is_free_of_duplicates)) -> Vec<u128> {
            entities.sort();
            entities.dedup();
            entities.into_iter().collect()
        }
    }

    pub fn partition_properties<E: Entity, T: Debug + Clone + Eq, C: ComponentCollection<E, T>>(
        collection: Vec<(E, T)>,
        partitioning: Arc<dyn PartitioningScheme<E>>,
    ) {
        collection_properties::<E, T, Partitioned<E, T,C>>(collection.clone());
        let components = C::from_iter(collection.clone());
        let is_empty = components.is_empty();
        let len = components.len();
        let partitioned = components.partition(&*partitioning);
        let partitioned = Partitioned::from(&partitioning, partitioned);
        assert_eq!(is_empty, partitioned.is_empty());
        assert_eq!(len, partitioned.len());
        for (e, t) in collection.iter() {
            assert_eq!(Some(*e), partitioned.lower_bound(*e));
            assert_eq!(*t, *partitioned.get_ref(*e).unwrap());
        }
        for (idx, (e, _)) in collection.iter().enumerate() {
            if idx > 0 && collection[idx - 1].0.increment() != collection[idx].0 {
                assert_eq!(Some(*e), partitioned.lower_bound(e.decrement()));
                assert!(partitioned.get_ref(e.decrement()).is_none());
            }
        }
        // TODO(apply);
        let consumed: Vec<(E, T)> = partitioned.consume().collect();
        assert_eq!(collection, consumed);
    }

    proptest::proptest! {
        #[test]
        fn partitioned_collection_properties(entities in arb_entities(), partitions in arb_partitions()) {
            let partitioning: Arc<dyn PartitioningScheme<u128>> = Arc::new(NopPartitioningScheme);
            partition_properties::<u128, usize, MutableComponentCollection<u128, usize>>(entities.clone(), partitioning);
            let partitioning: Arc<dyn PartitioningScheme<u128>> = Arc::new(VecPartitioningScheme::from(partitions));
            partition_properties::<u128, usize, MutableComponentCollection<u128, usize>>(entities, partitioning);
        }
    }
}
