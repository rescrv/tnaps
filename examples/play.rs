//! An example ECS system using tnaps.

use std::sync::Arc;

use tnaps::{
    system, system_parallel, ComponentChange, ComponentCollection, ComponentRef,
    CopyOnWriteComponentCollection, CopyOnWriteComponentRef, Entity as EntityTrait,
    MutableComponentCollection, MutableComponentRef, NopPartitioningScheme, Partitioned,
    PartitioningScheme, ThreadPool,
};

type Entity = u128;

struct MySystem1;

system! {
    MySystem1<Entity> {
        a: CopyOnWriteComponentCollection<u8>,
    }
}

impl MySystem1 {
    fn process(&self, entity: Entity, _: &mut CopyOnWriteComponentRef<u8>) {
        println!("processing: {}", entity);
    }
}

struct MySystem2;

system! {
    MySystem2<Entity> {
        a: CopyOnWriteComponentCollection<u8>,
        b: MutableComponentCollection<&'static str>,
    }
}

impl MySystem2 {
    fn process(
        &self,
        entity: Entity,
        _: &mut CopyOnWriteComponentRef<u8>,
        c2: &mut MutableComponentRef<&'static str>,
    ) {
        if entity == 2 {
            c2.unbind();
        }
        println!("processing: {}", entity);
    }
}

struct MySystem3;

system_parallel! {
    MySystem3<Entity> {
        a: CopyOnWriteComponentCollection<u8>,
        b: MutableComponentCollection<&'static str>,
        c: CopyOnWriteComponentCollection<f64>,
    }
}

impl MySystem3 {
    fn process(
        &self,
        entity: Entity,
        _: &mut CopyOnWriteComponentRef<u8>,
        _: &mut MutableComponentRef<&'static str>,
        c3: &mut CopyOnWriteComponentRef<f64>,
    ) {
        if entity == 3 {
            c3.update(|x| *x = 0.0);
        }
        println!("processing: {}", entity);
    }
}

fn main() {
    let mut collection1 = CopyOnWriteComponentCollection::from_iter(vec![
        (1u128, 42u8),
        (2u128, 69u8),
        (3u128, 99u8),
    ]);
    let mut collection2 = MutableComponentCollection::from_iter(vec![
        (1u128, "hello"),
        (2u128, "world"),
        (3u128, "!!!"),
    ]);
    let collection3 = CopyOnWriteComponentCollection::from_iter(vec![
        (1u128, std::f64::consts::PI),
        (2u128, std::f64::consts::E),
        (3u128, std::f64::consts::SQRT_2),
    ]);
    let sys1 = MySystem1;
    let sys2 = MySystem2;
    let sys3 = std::sync::Arc::new(MySystem3);
    println!("----");
    let (changes1,) = sys1.run(&mut collection1);
    assert!(changes1.is_empty());
    println!("----");
    let (changes1, changes2) = sys2.run(&mut collection1, &mut collection2);
    assert!(changes1.is_empty());
    collection2.apply(changes2);
    println!("collection2: {:?}", collection2);
    println!("----");
    let partitioning: Arc<dyn PartitioningScheme<Entity>> = Arc::new(NopPartitioningScheme);
    let collection1 = Partitioned::from(&partitioning, collection1.partition(&*partitioning));
    let mut collection2 = Partitioned::from(&partitioning, collection2.partition(&*partitioning));
    let mut collection3 = Partitioned::from(&partitioning, collection3.partition(&*partitioning));
    let thread_pool = ThreadPool::new("demo", 16);
    let (changes1, changes2, changes3) =
        sys3.run(&thread_pool, &collection1, &collection2, &collection3)();
    assert!(changes1.iter().all(|x| x.is_empty()));
    collection2.apply(changes2);
    collection3.apply(changes3);
    println!("collection2: {:?}", collection2);
    println!("collection3: {:?}", collection3);
}
