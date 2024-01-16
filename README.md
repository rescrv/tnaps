Totally Not a PlayStation
=========================

tnaps, pronounced tiː-næps, is a pure entity-component-system (ECS) framework
written in rust.  It was written to test the hypothesis that the same techniques
that allow games and their development process to scale with ECS will _also_
allow interactive applications to be built quickly and with good performance.
tnaps is a MVP to validate this hypothesis.

Entity-Component-System Overview
--------------------------------

Per Wikipedia, the entity-component-system paradigm has been around at least
since "Thief: The Dark Project" in 1998.  The system embraces the maxim,
"Composition over inheritance."  Entities exist, they have components, and
systems operate on said components.  In tnaps, an entity is a bag of components.
Arbitrary components can be added to or unbound from entities.  It can be
thought of as a pointer.  A component is pure data, or data with a small number
of localized methods that touch no other component.  A system operates on a set
of entities and their components and serves as the means by which entities with
components interact with one another.  Put another way that may be familiar to
database folks, the entities are row pointers, the components are columns, and
the system is a query engine.

The entity-component-system has a major advantage that can best be seen in this
table:

| Component | Entity 1 | Entity 2 | Entity 3 | ... | Entity N |
| --------- | -------- | -------- | -------- | --- | -------- |
| ABC       | ✓        | ✓        |          |     | ✓        |
| ...       |          |          |          |     |          |
| XYZ       |          | ✓        | ✓        |     | ✓        |

The component ABC is bound to entities 1, 2, and N, while the component XYZ is
bound to entities 2, 3, and N.  A system that operates on ABC must necessarily
visit entities 1, 2, and N, while a system that operates on XYZ must necessarily
visit 2, 3, and N.  A system that operates on both ABC and XYZ components need
only touch entities 2 and N.  And because entities 2 and N are different, the
system can operate on them in parallel as if it operated on them sequentially.

This observation is at the heart of tnaps.

This table illustrates the two degrees of freedom tnaps exhibits.  First, data
is partitioned by component.  A component is defined to be a self-contained
unit.  It could be a health meter, a position, or any other property of an
entity.  tnaps rewards small, isolated components.  Second, data is partitioned
by entity.  Systems that support concurrency and parallelism may operate on
different entities in parallel.

tnaps leverages rust's concurrency guarantees---namely send and sync marker
traits---to provide a framework by which code can be made concurrent.  Systems
whose state is entirely maintained within components can be trivially
parallelized with minimal code changes.

Getting Started
---------------

Let's jump right into an end-to-end example.  Here is a complete implementation
of a system that uses tnaps to process two components.

```
# use tnaps::{
#     system, system_parallel, ComponentChange, ComponentCollection,
#     ComponentRef, CopyOnWriteComponentCollection, CopyOnWriteComponentRef,
#     Entity as EntityTrait, MutableComponentCollection, MutableComponentRef,
#     NopPartitioningScheme, Partitioned, PartitioningScheme, ThreadPool,
# };
// Declare Entity to be a u128.
// Out of the box, tnaps supports u128, u64, and u32 entities.
type Entity = u128;

// Two sample componenents.
// They are just plain-old-data.
// Components can be practically any Rust type.
#[derive(Clone, Debug)]
struct ComponentAbc {
    x: u64,
    y: u64,
}

#[derive(Debug)]
struct ComponentXyz {
    z: f64,
}

// Let's declare a system that operates only on ABC.  It's totally
// permitted---and even desirable---for systems to have no state of their own.
// More complicated systems such as those we'll see later can have state, but it
// is subject to Rust's standard rules for concurrency.
struct SystemAbc;

// The system macro is the means by which we create a `run` method for our
// system.  We declare SystemAbc to have entity type Entity, and that it
// operates on a CopyOnWriteComponentCollection of type ComponentAbc.
system! {
    SystemAbc<Entity> {
        abc: CopyOnWriteComponentCollection<ComponentAbc>,
    }
}

// Our implementation of the system operates on an entity and a reference to a
// ComponentAbc.  Notice there's no mention of other entities.  We are operating
// on a single column from the above table.
impl SystemAbc {
    fn process(&self, entity: Entity, abc: &mut CopyOnWriteComponentRef<ComponentAbc>) {
        println!("processing: {}", entity);
    }
}

// Our system that operates entirely on a mutable component collection of
// ComponentXyz.
struct SystemXyz;

system! {
    SystemXyz<Entity> {
        xyz: MutableComponentCollection<ComponentXyz>,
    }
}

// Our implementation of SystemXyz.
impl SystemXyz {
    fn process(
        &self,
        entity: Entity,
        xyz: &mut MutableComponentRef<ComponentXyz>,
    ) {
        // We unbind entity two.
        if entity == 2 {
            xyz.unbind();
        }
        println!("processing: {}", entity);
    }
}

// A system that operates on both ABC and YXZ.
struct SystemAbcXyz;

system! {
    SystemAbcXyz<Entity> {
        abc: CopyOnWriteComponentCollection<ComponentAbc>,
        xyz: MutableComponentCollection<ComponentXyz>,
    }
}

// Our implementation of SystemAbcXyz.
impl SystemAbcXyz {
    fn process(
        &self,
        entity: Entity,
        abc: &mut CopyOnWriteComponentRef<ComponentAbc>,
        xyz: &mut MutableComponentRef<ComponentXyz>,
    ) {
        // We unbind entity two.
        if entity == 2 {
            xyz.unbind();
        }
        println!("processing: {}", entity);
    }
}

fn main() {
    let mut collection_abc = CopyOnWriteComponentCollection::from_iter(vec![
        (1u128, ComponentAbc { x: 10, y: 20 }),
        (3u128, ComponentAbc { x: 42, y: 43 }),
    ]);
    let mut collection_xyz = MutableComponentCollection::from_iter(vec![
        (2u128, ComponentXyz { z: std::f64::consts::PI }),
        (3u128, ComponentXyz { z: std::f64::consts::E }),
    ]);
    let sys1 = SystemAbc;
    let sys2 = SystemXyz;
    let sys3 = SystemAbcXyz;
    // Execute sys1 against abc.
    let (changes_abc,) = sys1.run(&mut collection_abc);
    assert!(changes_abc.is_empty());
    // Execute sys2 against xyz
    let (changes_xyz,) = sys2.run(&mut collection_xyz);
    assert!(!changes_xyz.is_empty());
    collection_xyz.apply(changes_xyz);
    // Execute sys3 against both abc and xyz.
    let (changes_abc, changes_xyz) = sys3.run(&mut collection_abc, &mut collection_xyz);
    collection_abc.apply(changes_abc);
    collection_xyz.apply(changes_xyz);
}
```

As may be intuitive, this will run `sys1` against entities 1 and 3, `sys2`
against entities 2 and 3, and `sys3` against their intersection: entity 3.

This is the power of the tnaps framework:  It takes the intersection of entities
and components and efficiently selects only those entities that are relevant to
a system.

This example showcases the majority of features of tnaps; it's intended to be
lightweight and consumable within a few hours.

We can make our example parallel as follows (only for `sys3`):

```
# use std::sync::Arc;
#
# use tnaps::{
#     system, system_parallel, ComponentChange, ComponentCollection,
#     ComponentRef, CopyOnWriteComponentCollection, CopyOnWriteComponentRef,
#     Entity as EntityTrait, MutableComponentCollection, MutableComponentRef,
#     NopPartitioningScheme, Partitioned, PartitioningScheme, ThreadPool,
# };
#
# type Entity = u128;
#
# #[derive(Clone, Debug)]
# struct ComponentAbc {
#     x: u64,
#     y: u64,
# }
#
# #[derive(Debug)]
# struct ComponentXyz {
#     z: f64,
# }
// A system that operates on both ABC and YXZ, in parallel.
struct SystemAbcXyz;

system_parallel! {
    SystemAbcXyz<Entity> {
        abc: CopyOnWriteComponentCollection<ComponentAbc>,
        xyz: MutableComponentCollection<ComponentXyz>,
    }
}

// Our implementation of SystemAbcXyz.  Unchanged from our first example.
impl SystemAbcXyz {
    fn process(
        &self,
        entity: Entity,
        abc: &mut CopyOnWriteComponentRef<ComponentAbc>,
        xyz: &mut MutableComponentRef<ComponentXyz>,
    ) {
        // We unbind entity two.
        if entity == 2 {
            xyz.unbind();
        }
        println!("processing: {}", entity);
    }
}

fn main() {
    let partitioning: Arc<dyn PartitioningScheme<Entity>> = Arc::new(NopPartitioningScheme) as _;
    let collection_abc = CopyOnWriteComponentCollection::from_iter(vec![
        (1u128, ComponentAbc { x: 10, y: 20 }),
        (3u128, ComponentAbc { x: 42, y: 43 }),
    ]);
    let mut collection_abc = Partitioned::from(&partitioning, collection_abc.partition(&*partitioning));
    let mut collection_xyz = MutableComponentCollection::from_iter(vec![
        (2u128, ComponentXyz { z: std::f64::consts::PI }),
        (3u128, ComponentXyz { z: std::f64::consts::E }),
    ]);
    let mut collection_xyz = Partitioned::from(&partitioning, collection_xyz.partition(&*partitioning));
    let sys3 = Arc::new(SystemAbcXyz);
    // Execute sys3 against both abc and xyz.
    let thread_pool = ThreadPool::new("example", 4);
    let waiter = sys3.run(&thread_pool, &mut collection_abc, &mut collection_xyz);
    let (changes_abc, changes_xyz) = waiter();
    collection_abc.apply(changes_abc);
    collection_xyz.apply(changes_xyz);
}
```

As we can see, our implementation of our system didn't change.  Our only changes
were to use the `system_parallel` macro instead of `system` and to wrap each
entity as a `Partitioned` collection.  We used a `NopPartitioningScheme`, but
it's just as valid to use another partitioning scheme.

That's all it takes to convert a system from non-concurrent to concurrent
behavior.

Component Collections
---------------------

tnaps' design centers around components and their collections.  A singular
component collection is intended to map from entities to components in an
efficient manner that allows us to join multiple components together in a single
system.  tnaps ships four component collections out of the box, that reasonably
cover more use cases.  It is always possible to manually implement the
`ComponentCollection` trait to implement a custom collection.

The included collections are:

- Copy On Write:  This component collection returns all its changes for use with
  the `apply` collection call.  It will then rewrite the entire collection to
  maintain order of the components according to their entities.

- Insert Optimized:  This component collection is optimized for workloads that
  bind components to entities or update entities infrequently.  Unlike the
  copy-on-write collection, which amortizes update cost, the insert-optimized
  collection exposes a method for quick inserting of single elements, such that
  the average cost of inserting an element is greater than other collections,
  but there is no amortization of the cost.

- Mutable:  This component collection is useful for when components won't be
  inserted frequently, but will instead be updated in-place.  Binding and
  unbinding of components from entities is only slightly (nanoseconds) more
  expensive than the copy-on-write collection and is significantly less costly
  than the insert-optimized collection.

- Partitioned:  This component collection wraps another type of component
  collection and fragments it into partitions according to a partitioning
  scheme.  This is useful because different threads can operate on different
  partitions concurrently.  Consequently, the partitioned collection is a
  prerequisite for parallel systems.

Common Patterns
---------------

This section outlines some common patterns to get the most out of tnaps.

- When working with systems that operate on different components, it is
  desirable to order the components by increasing cardinality.  The fewer items
  in a component, the more it can prune entities from consideration in other
  components.  For example, it might make sense to order the player-oriented
  components before the bullet-oriented components in a system that tries to
  detect when players get hit by a bullet.

- The copy-on-write component collection is `Send` and `Sync`, meaning that it
  can be used by multiple systems in parallel.  This allows an application to
  aggregate changes from multiple parallel systems and apply them as one batch
  at the end of a main-loop iteration.

- It's tempting to use the `run_subset` feature of systems to subselect entities
  on which to operate.  This is marginally more efficient than using marker
  components to prune for a system.  Create a marker component for each entity
  that would be passed to `run_subset` and trust the system to run in parallel.
  `run_subset` is unable to work on parallel systems.

- `run_subset` is useful for enumerating entities out of order.  This allows a
  sequential system (perhaps collision detection) to pre-sort the entities by a
  predicate and then be guaranteed that the entities will be enumerated in
  arbitrary order.  This can be a win for systems that need to consider entities
  with locality that doesn't match the total order across entities.

- Binding new components to entities using an insert-optimized map has a cost.
  Unless the entity needs to be immediately bound, or there are few updates per
  tick, it is more efficient to use a copy-on-write or mutable component
  collection and apply all updates as a single batch.

- All entities exist all the time, they just may not be bound to any components,
  and thus omitted from consideration.  This eliminates a central entity
  registry.

- It is possible to make an entity type that's not an integer.  This is an
  anti-pattern within tnaps.  Use the built-in entity types for most
  applications.

- Generate entities using modular arithmetic.  Pick a number that is co-prime to
  the number of entities possible, start at an arbitrary random number, and then
  use `wrapping_mul` to enumerate the entities.  By the properties of modular
  arithmetic, all entities will be enumerated before recycling.  To efficiently
  pick a coprime, select small primes _other than two_ at random and multiply
  them together until the next-selected prime would overflow your entity type.
  By properties of the prime factorization, the number will be co-prime to two
  to the one-hundred-twenty-eight.

- Alternatively, use u128 as an entity and generate a UUID.

- Use sync components to allow in-place updates, even for the
  `CopyOnWriteComponentCollection`.

- `FastEntityMap` is slower to construct, but significantly faster to query.
  There's an open TODO to make all component collections generic over EntityMap.

- Consider the main loop to encode a DAG of data flow.  This will help model
  systems better and is akin to a data flow engine.

- Systems can be viewed as database-like "joins" of components.  Where different
  entities must interact, there exists a minimal system that joins their
  component data.  The `process` method of the system should collect the data
  for the join, and then the system should perform the join either in the
  process call, or in a separate call after `run` or `run_parallel` completes.

- "Signaling" can be accomplished by having listeners that get joined against
  notifiers.

- "Zones" of entities can be made where each zone has an independent set of
  collections and systems.  To move entities between zones, have a system that
  completely unbinds an entity from one zone's collections and binds it in all
  the requisite component collections in the other zone.  There's an open TODO
  to unify this with partitioning.

- Input is a component.

- Create marker components to join entities against recent activity.

- Persistent systems should detect changes to components and persist the
  changes.  This can be used to get durability of the entire
  entity-component-system application.  It is an anti-pattern to try and keep
  all state from the database in tnaps.  Instead, retain primary keys and copy
  them out to systems that return references into the database.

Design Choices
--------------

There are many design choices intentionally baked into tnaps to make it both
functional and small.  Functionality is preferred over size, but being able to
keep the entire framework in one's head has its advantages.

- Each component instance is bound to exactly one entity.  This is a fundamental
  assumption of tnaps and enables its efficient pruning of entities and
  components.  It is always possible to shove shared state behind an `Arc` to
  have components that share state across entities.

- The interface for systems was intentionally designed to process one entity at
  a time.  This allows parallelization across entities, without leaking any
  details of said parallelization.  In the future, SIMD-like systems will be
  possible.

- tnaps assumes all component collections will be sorted according to an entity
  map.  This enables efficient scanning of all components to find the
  intersection of entities that exist in two or more component collections.

- No main is included with tnaps.  This is highly application dependent.  It
  will look like a main function with a loop that repeatedly runs systems on
  components.  The similarities stop there, so tnaps ships nothing.

- No systems are included with tnaps.  Common components for large-scale
  interactive applications will ship separately.

- Entities always exist.  Consequently it is never an error to bind new
  components to entities.  This assumption enables performance.

- Rust was chosen for tnaps because of its safety features, largely Send + Sync.

- Most tnaps tests use random, property-based testing.

- Entities are chosen to be pure, and devoid of meaningful data.  Consequently
  they are lightweight and easy to copy.  This is why `Entity` must implement
  `Copy`.

- Components should be pure data.  Different data should be different
  components.  Multiple sparse collections should be as efficient to process as
  one giant, inheritance-based collection.  There are no requirements on
  component in general (except Debug and Clone where appropriate).

- Systems should be code plus system state.  tnaps places no assumptions on what
  they look like.  This is freedom, not a restriction.

- tnaps' system and component interface allows flexible scheduling.  It is an
  open TODO to make the scheduling dynamically repartition collections according
  to execution time of the partitions.

Napkin Computations
-------------------

Let's pretend we're in a bar, working on a napkin, to figure out what's possible
for tnaps.

For starters, let's clear the air:  One million is not a big number for a
computer.

```ignore
ENTITIES = 1_000_000
```

And modern computers are ridiculously large for low cost.  For $8-$11/hour we
can get a machine with more than 100 cores and hundreds of gigabytes of memory.

Concretely, we have two reference points that can be leased from Amazon AWS:

|              | c7i.48xl  | i4i.32xl  |
| ------------ | --------- | --------- |
| CPU Cores    | 192       | 128       |
| Memory       | 384 GB    | 1024 GB   |
| Network      | 50 Gbit/s | 75 Gbit/s |
| Attached SSD | N/A       | 8 x SSD   |
| Price/Hour   | $8.568    | $10.982   |

These machines are ridiculously over-powered and cost relatively little compared
to the costs of a regular software-oriented business.  If we can fit our entire
application in tnaps (our central hypothesis), these machines should be able to
fit the bill.

Let's start with compute.  tnaps is built around a loop that executes once per
tick.  What's a tick?  Let's make it variable and see how much compute time
falls out for a c7i.48xl:

| Tick Interval | Compute | Per Entity |
| ------------- | ------- | ---------- |
| 1/60 second   | 3.2s    | 3.2µs      |
| 1/30 second   | 6.4s    | 6.4µs      |
| 1 second      | 192s    | 192µs      |
| 5 seconds     | 950s    | 960µs      |
| 15 seconds    | 48min   | 2.88ms     |
| 60 seconds    | 192min  | 11.5ms     |

I've waited longer than a minute for my rideshare and food delivery apps to
acknowledge my order.  What are they doing with that time?

Of course, I'm glossing over durability.  Let's continue using our c7i.48xl
machine.  It provides 40Gbit/s of bandwidth to Amazon EBS.

| Recovery Volume | Recovery Time |
| --------------- | ------------- |
| 64 B/entity     | 12.8ms        |
| 128 B/entity    | 25.6ms        |
| 256 B/entity    | 51.2ms        |
| 512 B/entity    | 102ms         |
| 1 KB/entity     | 204ms         |
| ...             | ...           |
| 64 KB/entity    | 13.1s         |
| 1 MB/entity     | 209s          |

Of course, these are perfect numbers, and 209s of downtime is nothing to
write-off.  Three minutes of downtime (which will happen on every application
restart) might or might not be acceptable.  But the full state doesn't have to
materialize on startup.  It can materialize slowly as requests trickle in.

Wrapping Up
-----------

I hold firmly to my hypothesis that entity-component-system is THE way to build
apps where data fits on a single machine.  Looking at SEC filings for common
interactive apps in the gig economy, I believe 1e6 active entities to be a
reasonable target point for applications.  There's unlikely to ever be that many
active entities in a single regulatory domain or geo-fenced area.

tnaps provides an interface in which components and systems are both trivial to
implement and easy to test.  Testing a system is as easy as instantiating the
system and calling `process` on it in the same way that tnaps itself would.

Circling back to my hypothesis, there is one location that sets the data flow of
the application (main).  From there, separate concerns can be cleanly isolated
into systems over re-usable components.  Adding a functionality can be done by
adding the data (components) and implementing the code (system).  Critically,
this only requires coordination with other systems that touch the same
components, and Rust + tnaps takes care of that coordination for the most part.
You _know_ without a doubt what components are touched by what systems by
looking for the `run` calls into your systems.  Similarly, you know what data
changes because of guarantees made by the type system.

Looking Forward
---------------

I want to validate the hypothesis behind tnaps, and take away the assumption of
operating on a single-computer domain without persistence.

Disclaimer
----------

As the title of this document would imply, tnaps is not affiliated with Sony.  I
love my PlayStation and tnax and tnan don't have the same ring as tnaps.
