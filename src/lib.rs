#![doc = include_str!("../README.md")]

mod base64;
mod component;
mod entity;
mod partitioning;
mod thread_pool;

pub use component::{
    ComponentChange, ComponentCollection, ComponentRef, CopyOnWriteComponentCollection,
    CopyOnWriteComponentRef, InsertOptimizedComponentCollection, InsertOptimizedComponentRef,
    MutableComponentCollection, MutableComponentRef,
};
pub use entity::{
    Entity, EntityMap, FastEntityMap, FastEntityMapIntoIterator, FastEntityMapIterator,
    VecEntityMap,
};
pub use partitioning::{
    NopPartitioningScheme, Partitioned, PartitioningScheme, VecPartitioningScheme,
};
pub use thread_pool::{ThreadPool, WorkUnit};

////////////////////////////////////////////// system //////////////////////////////////////////////

/// Define a run method for the described system.  The generated method will take a list of args
/// that are component collections and return a tuple of vectors of changes for each component
/// collection.  It is up to the user to subsequently pass this state to the `apply` method of the
/// component collections.
#[macro_export]
macro_rules! system {
    ($system:ident <$entity:ty> {}) => {
        compile_error!("A system operates on 1 or more component collections.  Found: 0.");
    };

    ($system:ident <$entity:ty> { $($arg:ident: $collection:ident <$t:ty>,)+ }) => {
        impl $system {
            fn run(&self, $($arg: &mut $crate::$collection<$entity, $t>),+) -> ($(Vec<($entity, ComponentChange<$t>)>,)+) {
                #[derive(Default)]
                struct Results {
                    $($arg: Vec<($entity, ComponentChange<$t>)>,)+
                }
                let mut target = <$entity as Default>::default();
                let mut results = Results::default();
                'zipper: loop {
                    $(
                        let Some(lb) = $arg.lower_bound(target) else {
                            break 'zipper;
                        };
                        if lb > target {
                            target = lb;
                            continue 'zipper;
                        }
                    )+
                    // SAFETY(rescrv):  We know that target is an entity that exists in all args.
                    $(let mut $arg = $arg.get_ref(target).expect("target should be present");)+
                    self.process(target, $(&mut $arg),+);
                    // Gather changes.
                    $(
                        let $arg = $arg.change();
                        if !$arg.is_no_change() {
                            results.$arg.push((target, $arg));
                        }
                    )+
                    // Make it so we move past this entity.
                    target = target.increment();
                }
                ($(results.$arg,)+)
            }

            fn run_subset(&self, entities: &[$entity], $($arg: &mut $crate::$collection<$entity, $t>),+) -> ($(Vec<($entity, ComponentChange<$t>)>,)+) {
                #[derive(Default)]
                struct Results {
                    $($arg: Vec<($entity, ComponentChange<$t>)>,)+
                }
                let mut results = Results::default();
                for target in entities.iter() {
                    $(
                        let Some(mut $arg) = $arg.get_ref(target.clone()) else {
                            continue;
                        };
                    )+
                    self.process(target.clone(), $(&mut $arg),+);
                    // Gather changes.
                    $(
                        let $arg = $arg.change();
                        if !$arg.is_no_change() {
                            results.$arg.push((target.clone(), $arg));
                        }
                    )+
                }
                $(results.$arg.sort_by_key(|x| x.0);)+
                ($(results.$arg,)+)
            }
        }
    };
}

/// Define a run method for the described system that operates in parallel using a thread pool.
/// The generated method will take a list of args that are component collections and return a tuple
/// of vectors of changes for each component collection.  It is up to the user to subsequently pass
/// this state to the `apply` method of the component collections.
#[macro_export]
macro_rules! system_parallel {
    ($system:ident <$entity:ty> {}) => {
        compile_error!("A system operates on 1 or more component collections.  Found: 0.");
    };

    ($system:ident <$entity:ty> { $($arg:ident: $collection:ident <$t:ty>,)+ }) => {
        impl $system {
            fn run(self: std::sync::Arc<Self>, thread_pool: &ThreadPool,
                   $($arg: &$crate::Partitioned<$entity, $t, $crate::$collection<$entity, $t>>),+)
                -> impl FnOnce() -> ($(Vec<Vec<($entity, ComponentChange<$t>)>>,)+)
            {
                use std::sync::atomic::{AtomicUsize, Ordering};
                use std::sync::{Arc, Condvar, Mutex};
                let system = Arc::clone(&self);
                #[derive(Default)]
                struct Intermediate {
                    $($arg: Vec<($entity, ComponentChange<$t>)>,)+
                }
                #[derive(Default)]
                struct Results {
                    $($arg: Vec<Vec<($entity, ComponentChange<$t>)>>,)+
                }
                struct WorkInput {
                    $($arg: Arc<$crate::$collection<$entity, $t>>,)+
                }
                impl WorkInput {
                    fn gather_results(&self, system: Arc<$system>) -> Intermediate {
                        let mut target = <$entity as Default>::default();
                        let mut results = Intermediate::default();
                        'zipper: loop {
                            $(
                                let Some(lb) = self.$arg.lower_bound(target) else {
                                    break 'zipper;
                                };
                                if lb > target {
                                    target = lb;
                                    continue 'zipper;
                                }
                            )+
                            // SAFETY(rescrv):  We know that target is an entity that exists in all args.
                            $(let mut $arg = self.$arg.get_ref(target).expect("target should be present");)+
                            system.process(target, $(&mut $arg),+);
                            // Gather changes.
                            $(
                                let $arg = $arg.change();
                                if !$arg.is_no_change() {
                                    results.$arg.push((target, $arg));
                                }
                            )+
                            // Make it so we move past this entity.
                            target = target.increment();
                        }
                        results
                    }
                }
                struct AggregatePartitions {
                    partitions: Mutex<Vec<Option<Intermediate>>>,
                    done: AtomicUsize,
                    wait: Condvar,
                }
                impl AggregatePartitions {
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
                        }
                    }

                    fn done(&self, partition: usize, results: Intermediate) {
                        let len = {
                            let mut partitions = self.partitions.lock().unwrap();
                            if partitions[partition].is_none() {
                                // SAFETY(rescrv):  We need this Some(_) assignment to be the only
                                // one, and it must be 1:1 with the fetch_add.
                                partitions[partition] = Some(results);
                                self.done.fetch_add(1, Ordering::Relaxed);
                            }
                            partitions.len()
                        };
                        if len == self.done.load(Ordering::Relaxed) {
                            self.wait.notify_all();
                        }
                    }

                    fn wait(&self) -> ($(Vec<Vec<($entity, ComponentChange<$t>)>>,)+) {
                        let mut partitions = self.partitions.lock().unwrap();
                        while self.done.load(Ordering::Relaxed) < partitions.len() {
                            partitions = self.wait.wait(partitions).unwrap();
                        }
                        let mut results = Results::default();
                        for partition in partitions.iter_mut() {
                            // SAFETY(rescrv):  We wait until all partitions have been set.
                            // About 20 lines north of here we set Some(results) atomic with
                            // incrementing of the done count.
                            let mut partition = partition.take().unwrap();
                            $(results.$arg.push(partition.$arg);)+
                        }
                        ($(results.$arg,)+)
                    }
                }
                $(let ptr = $arg.partitioning_scheme();)+
                $(
                    if !Arc::ptr_eq(ptr, $arg.partitioning_scheme()) {
                        panic!("parallel system run with different partitioning schemes");
                    }
                )+
                // NOTE(rescrv):  There's always one more partition in the collection than the
                // partitioning scheme.  This is so that we capture everything greater-equal than
                // the last partition listed (or, if there are no partitions).
                let partitions = ptr.len() + 1;
                let agg = Arc::new(AggregatePartitions::new(partitions));
                for partition in 0..partitions {
                    $(
                        let Some($arg) = $arg.get_partition_by_index(partition) else {
                            agg.done(partition, Intermediate::default());
                            continue;
                        };
                    )+
                    let work_input = WorkInput {
                        $($arg,)+
                    };
                    let system = Arc::clone(&system);
                    let agg = Arc::clone(&agg);
                    let work_unit: Box<$crate::WorkUnit> = Box::new(move || {
                        let results = work_input.gather_results(system);
                        agg.done(partition, results);
                    });
                    thread_pool.enqueue(work_unit);
                }
                move || {
                    agg.wait()
                }
            }
        }
    };
}

/////////////////////////////////////////////// tests //////////////////////////////////////////////

#[cfg(test)]
mod tests {
    extern crate proptest;

    use proptest::strategy::Strategy;

    proptest::prop_compose! {
        pub fn arb_entity()(entity in (u128::MIN..u128::MAX).prop_filter("nonzero", |x| *x != 0).prop_filter("nonmax", |x| *x != u128::MAX)) -> u128 {
            entity
        }
    }

    pub fn is_free_of_duplicates(entities: &Vec<u128>) -> bool {
        let mut entities = entities.clone();
        let len = entities.len();
        entities.sort();
        entities.dedup();
        entities.len() == len
    }
}
