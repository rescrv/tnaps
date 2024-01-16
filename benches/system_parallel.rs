use std::fmt::Debug;
use std::sync::Arc;

use guacamole::combinators::*;
use guacamole::{FromGuacamole, Guacamole};
use statslicer::{benchmark, black_box, statslicer_main, Bencher, Parameter, Parameters};

use tnaps::{
    system_parallel, ComponentChange, ComponentCollection, ComponentRef,
    CopyOnWriteComponentCollection, CopyOnWriteComponentRef, Entity, Partitioned,
    PartitioningScheme, ThreadPool, VecPartitioningScheme,
};

////////////////////////////////////////////// System1 /////////////////////////////////////////////

struct System1;

system_parallel! {
    System1<u128> {
        a: CopyOnWriteComponentCollection<u128>,
    }
}

impl System1 {
    fn process(&self, e: u128, a: &mut CopyOnWriteComponentRef<u128>) {
        black_box(e);
        black_box(a);
    }
}

////////////////////////////////////////////// System2 /////////////////////////////////////////////

struct System2;

system_parallel! {
    System2<u128> {
        a: CopyOnWriteComponentCollection<u128>,
        b: CopyOnWriteComponentCollection<u128>,
    }
}

impl System2 {
    fn process(
        &self,
        e: u128,
        a: &mut CopyOnWriteComponentRef<u128>,
        b: &mut CopyOnWriteComponentRef<u128>,
    ) {
        black_box(e);
        black_box(a);
        black_box(b);
    }
}

////////////////////////////////////////////// System3 /////////////////////////////////////////////

struct System3;

system_parallel! {
    System3<u128> {
        a: CopyOnWriteComponentCollection<u128>,
        b: CopyOnWriteComponentCollection<u128>,
        c: CopyOnWriteComponentCollection<u128>,
    }
}

impl System3 {
    fn process(
        &self,
        e: u128,
        a: &mut CopyOnWriteComponentRef<u128>,
        b: &mut CopyOnWriteComponentRef<u128>,
        c: &mut CopyOnWriteComponentRef<u128>,
    ) {
        black_box(e);
        black_box(a);
        black_box(b);
        black_box(c);
    }
}

//////////////////////////////////////////// Parameters ////////////////////////////////////////////

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum Order {
    #[default]
    LargestFirst,
    SmallestFirst,
}

#[derive(Debug, Default, Eq, PartialEq)]
struct SystemParameters {
    components: usize,
    collections: usize,
    ordering: Order,
    threads: usize,
}

impl Parameters for SystemParameters {
    fn params(&self) -> Vec<(&'static str, Parameter)> {
        let ordering = match &self.ordering {
            Order::LargestFirst => "largest_first",
            Order::SmallestFirst => "smallest_first",
        };
        vec![
            ("components", Parameter::Integer(self.components as u64)),
            ("collections", Parameter::Integer(self.collections as u64)),
            ("ordering", Parameter::Text(ordering.to_string())),
            ("threads", Parameter::Integer(self.threads as u64)),
            ("parallel", Parameter::Bool(true)),
        ]
    }
}

/////////////////////////////////////////////// utils //////////////////////////////////////////////

const COLLECTION_SET: usize = 2451481905;

fn collection<E: Entity + FromGuacamole<()>, T: Debug + FromGuacamole<()>>(
    size: usize,
    guac: &mut Guacamole,
) -> CopyOnWriteComponentCollection<E, T> {
    let mut entities: Vec<E> = to_vec(
        constant(size),
        set_element(unique_set(size, COLLECTION_SET), from_seed(any::<E>)),
    )(guac);
    entities.sort();
    entities.dedup();
    let values: Vec<T> = to_vec(constant(entities.len()), any::<T>)(guac);
    let components: Vec<_> = std::iter::zip(entities, values).collect();
    CopyOnWriteComponentCollection::from_iter(components)
}

fn partitioning<E: Entity + FromGuacamole<()> + 'static>(
    size: usize,
    guac: &mut Guacamole,
) -> Arc<dyn PartitioningScheme<E>> {
    let mut entities: Vec<E> = to_vec(
        constant(size),
        set_element(unique_set(size, COLLECTION_SET), from_seed(any::<E>)),
    )(guac);
    entities.sort();
    entities.dedup();
    Arc::new(VecPartitioningScheme::from(entities))
}

fn system_parallel<
    T,
    F1: FnMut(&SystemParameters, &mut Bencher) -> T,
    F2: FnMut(usize, T, &ThreadPool),
>(
    params: &SystemParameters,
    b: &mut Bencher,
    thread_pool: &ThreadPool,
    mut f1: F1,
    mut f2: F2,
) {
    let args = f1(params, b);
    let size = b.size();
    b.run(|| {
        black_box(f2(size, black_box(args), thread_pool));
    });
}

///////////////////////////////////////////// benchmark ////////////////////////////////////////////

fn bench_system(params: &SystemParameters, b: &mut Bencher) {
    fn generate_components_1(
        params: &SystemParameters,
        b: &mut Bencher,
    ) -> Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>> {
        let mut guac = Guacamole::new(b.seed());
        let partitioning = partitioning(params.components >> 10, &mut guac);
        Partitioned::from(
            &partitioning,
            collection(params.components, &mut guac).partition(partitioning.as_ref()),
        )
    }
    fn run_system_1(
        iter: usize,
        mut collection: Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        thread_pool: &ThreadPool,
    ) {
        let system = Arc::new(System1);
        for _ in 0..iter {
            black_box(system.clone().run(thread_pool, black_box(&mut collection)))();
        }
    }
    fn generate_components_2_smallest_first(
        params: &SystemParameters,
        b: &mut Bencher,
    ) -> (
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
    ) {
        let mut guac = Guacamole::new(b.seed());
        let partitioning = partitioning(params.components >> 10, &mut guac);
        (
            Partitioned::from(
                &partitioning,
                collection(params.components, &mut guac).partition(partitioning.as_ref()),
            ),
            Partitioned::from(
                &partitioning,
                collection(8 * params.components, &mut guac).partition(partitioning.as_ref()),
            ),
        )
    }
    fn generate_components_2_largest_first(
        params: &SystemParameters,
        b: &mut Bencher,
    ) -> (
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
    ) {
        let mut guac = Guacamole::new(b.seed());
        let partitioning = partitioning(params.components >> 10, &mut guac);
        (
            Partitioned::from(
                &partitioning,
                collection(8 * params.components, &mut guac).partition(partitioning.as_ref()),
            ),
            Partitioned::from(
                &partitioning,
                collection(params.components, &mut guac).partition(partitioning.as_ref()),
            ),
        )
    }
    fn run_system_2(
        iter: usize,
        args: (
            Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
            Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        ),
        thread_pool: &ThreadPool,
    ) {
        let (mut collection1, mut collection2) = args;
        let system = Arc::new(System2);
        for _ in 0..iter {
            black_box(system.clone().run(
                thread_pool,
                black_box(&mut collection1),
                black_box(&mut collection2),
            ))();
        }
    }
    fn generate_components_3_smallest_first(
        params: &SystemParameters,
        b: &mut Bencher,
    ) -> (
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
    ) {
        let mut guac = Guacamole::new(b.seed());
        let partitioning = partitioning(params.components >> 10, &mut guac);
        (
            Partitioned::from(
                &partitioning,
                collection(params.components, &mut guac).partition(partitioning.as_ref()),
            ),
            Partitioned::from(
                &partitioning,
                collection(8 * params.components, &mut guac).partition(partitioning.as_ref()),
            ),
            Partitioned::from(
                &partitioning,
                collection(64 * params.components, &mut guac).partition(partitioning.as_ref()),
            ),
        )
    }
    fn generate_components_3_largest_first(
        params: &SystemParameters,
        b: &mut Bencher,
    ) -> (
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
    ) {
        let mut guac = Guacamole::new(b.seed());
        let partitioning = partitioning(params.components >> 10, &mut guac);
        (
            Partitioned::from(
                &partitioning,
                collection(64 * params.components, &mut guac).partition(partitioning.as_ref()),
            ),
            Partitioned::from(
                &partitioning,
                collection(8 * params.components, &mut guac).partition(partitioning.as_ref()),
            ),
            Partitioned::from(
                &partitioning,
                collection(params.components, &mut guac).partition(partitioning.as_ref()),
            ),
        )
    }
    fn run_system_3(
        iter: usize,
        args: (
            Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
            Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
            Partitioned<u128, u128, CopyOnWriteComponentCollection<u128, u128>>,
        ),
        thread_pool: &ThreadPool,
    ) {
        let (mut collection1, mut collection2, mut collection3) = args;
        let system = Arc::new(System3);
        for _ in 0..iter {
            black_box(system.clone().run(
                thread_pool,
                black_box(&mut collection1),
                black_box(&mut collection2),
                black_box(&mut collection3),
            ))();
        }
    }
    let thread_pool = ThreadPool::new("tnaps-benchmark", params.threads);
    match &params.collections {
        1 => system_parallel(params, b, &thread_pool, generate_components_1, run_system_1),
        2 => match &params.ordering {
            Order::LargestFirst => system_parallel(
                params,
                b,
                &thread_pool,
                generate_components_2_largest_first,
                run_system_2,
            ),
            Order::SmallestFirst => system_parallel(
                params,
                b,
                &thread_pool,
                generate_components_2_smallest_first,
                run_system_2,
            ),
        },
        3 => match &params.ordering {
            Order::LargestFirst => system_parallel(
                params,
                b,
                &thread_pool,
                generate_components_3_largest_first,
                run_system_3,
            ),
            Order::SmallestFirst => system_parallel(
                params,
                b,
                &thread_pool,
                generate_components_3_smallest_first,
                run_system_3,
            ),
        },
        _ => {
            panic!("{} collections is not supported", params.collections);
        }
    }
    thread_pool.shutdown();
}

benchmark! {
    name = system_run;
    SystemParameters {
        components in &[65536],
        collections in &[1, 2, 3],
        ordering in &[Order::SmallestFirst, Order::LargestFirst],
        threads in &[2],
    }
    bench_system,
}

statslicer_main! {
    system_run,
}
