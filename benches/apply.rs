use std::fmt::Debug;

use guacamole::combinators::*;
use guacamole::{FromGuacamole, Guacamole};
use statslicer::{benchmark, black_box, statslicer_main, Bencher, Parameter, Parameters};

use tnaps::{
    ComponentChange, ComponentCollection, CopyOnWriteComponentCollection, Entity,
    InsertOptimizedComponentCollection, MutableComponentCollection,
};

//////////////////////////////////////////// EntityType ////////////////////////////////////////////

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum EntityType {
    #[default]
    U128,
    U64,
    U32,
}

impl EntityType {
    fn as_str(&self) -> u64 {
        match self {
            Self::U128 => "u128",
            Self::U64 => "u64",
            Self::U32 => "u32",
        }
    }
}

///////////////////////////////////////////// Alignment ////////////////////////////////////////////

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum Alignment {
    #[default]
    Align16,
    Align32,
    Align64,
}

impl Alignment {
    fn as_u64(&self) -> u64 {
        match self {
            Self::Align64 => 64,
            Self::Align32 => 32,
            Self::Align16 => 16,
        }
    }
}

////////////////////////////////////////// CollectionType //////////////////////////////////////////

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum CollectionType {
    #[default]
    CopyOnWrite,
    InsertOptimized,
    Mutable,
}

impl CollectionType {
    fn as_str(&self) -> String {
        match self {
            CollectionType::CopyOnWrite => "cow",
            CollectionType::InsertOptimized => "ins",
            CollectionType::Mutable => "mut",
        }
        .to_string()
    }
}

/////////////////////////////////////////////// utils //////////////////////////////////////////////

fn collection<
    E: Entity + FromGuacamole<()>,
    T: Debug + FromGuacamole<()>,
    C: ComponentCollection<E, T>,
>(
    size: usize,
    guac: &mut Guacamole,
) -> (Vec<E>, C) {
    let mut entities: Vec<E> = to_vec(
        constant(size),
        set_element(enumerate(), from_seed(any::<E>)),
    )(guac);
    entities.sort();
    entities.dedup();
    let values: Vec<T> = to_vec(constant(entities.len()), any::<T>)(guac);
    let components: Vec<_> = std::iter::zip(entities.clone(), values).collect();
    (entities, C::from_iter(components))
}

fn changes<E: Entity + FromGuacamole<()>, T: Debug + FromGuacamole<()>>(
    size: usize,
    exists_probability: f32,
    entities: &[E],
    guac: &mut Guacamole,
) -> Vec<(E, ComponentChange<T>)> {
    let gen = |guac: &mut Guacamole| {
        if prob(exists_probability)(guac) {
            select(range_to(entities.len()), entities)(guac)
        } else {
            any::<E>(guac)
        }
    };
    let mut entities: Vec<E> = to_vec(constant(size), gen)(guac);
    let values: Vec<ComponentChange<T>> = to_vec(constant(entities.len()), any::<T>)(guac)
        .into_iter()
        .map(|x| ComponentChange::Value(x))
        .collect();
    std::iter::zip(entities, values).collect()
}

////////////////////////////////////////////// Aligned /////////////////////////////////////////////

#[derive(Clone, Debug)]
#[repr(C, align(16))]
struct Aligned16 {
    x: [u64; 2],
}

impl FromGuacamole<()> for Aligned16 {
    fn from_guacamole(_: &mut (), guac: &mut Guacamole) -> Self {
        Self { x: any(guac) }
    }
}

#[derive(Clone, Debug)]
#[repr(C, align(32))]
struct Aligned32 {
    x: [u64; 4],
}

impl FromGuacamole<()> for Aligned32 {
    fn from_guacamole(_: &mut (), guac: &mut Guacamole) -> Self {
        Self { x: any(guac) }
    }
}

#[derive(Clone, Debug)]
#[repr(C, align(64))]
struct Aligned64 {
    x: [u64; 8],
}

impl FromGuacamole<()> for Aligned64 {
    fn from_guacamole(_: &mut (), guac: &mut Guacamole) -> Self {
        Self { x: any(guac) }
    }
}

////////////////////////////////////////// ApplyParameters /////////////////////////////////////////

#[derive(Debug, Default)]
struct ApplyParameters {
    components: usize,
    mutate_probability: f32,
    entity_type: EntityType,
    alignment: Alignment,
    collection_type: CollectionType,
}

impl Parameters for ApplyParameters {
    fn params(&self) -> Vec<(&'static str, Parameter)> {
        vec![
            ("components", Parameter::Integer(self.components as u64)),
            ("mutate", Parameter::Float(self.mutate_probability as f64)),
            ("entity_type", Parameter::Integer(self.entity_type.as_str())),
            ("alignment", Parameter::Integer(self.alignment.as_u64())),
            (
                "collection_type",
                Parameter::Text(self.collection_type.as_str()),
            ),
        ]
    }
}

//////////////////////////////////////////// bench_apply ///////////////////////////////////////////

fn bench_apply_inner<
    E: Entity + FromGuacamole<()>,
    T: Debug + FromGuacamole<()>,
    C: ComponentCollection<E, T>,
>(
    params: &ApplyParameters,
    b: &mut Bencher,
) {
    let mut guac = Guacamole::new(b.seed());
    let (entities, mut collection): (Vec<E>, C) = collection(params.components, &mut guac);
    let changes: Vec<(E, ComponentChange<T>)> =
        changes(b.size(), params.mutate_probability, &entities, &mut guac);
    b.run(|| {
        black_box(collection.apply(black_box(changes)));
    });
}

fn bench_apply_component_type<
    E: Entity + FromGuacamole<()>,
    T: Clone + Debug + FromGuacamole<()>,
>(
    params: &ApplyParameters,
    b: &mut Bencher,
) {
    match params.collection_type {
        CollectionType::CopyOnWrite => {
            bench_apply_inner::<E, T, CopyOnWriteComponentCollection<E, T>>(params, b)
        }
        CollectionType::InsertOptimized => {
            bench_apply_inner::<E, T, InsertOptimizedComponentCollection<E, T>>(params, b)
        }
        CollectionType::Mutable => {
            bench_apply_inner::<E, T, MutableComponentCollection<E, T>>(params, b)
        }
    }
}

fn bench_apply_alignment<E: Entity + FromGuacamole<()>>(params: &ApplyParameters, b: &mut Bencher) {
    match params.alignment {
        Alignment::Align16 => bench_apply_component_type::<E, Aligned16>(params, b),
        Alignment::Align32 => bench_apply_component_type::<E, Aligned32>(params, b),
        Alignment::Align64 => bench_apply_component_type::<E, Aligned64>(params, b),
    }
}

fn bench_apply(params: &ApplyParameters, b: &mut Bencher) {
    match params.entity_type {
        EntityType::U128 => bench_apply_alignment::<u128>(params, b),
        EntityType::U64 => bench_apply_alignment::<u64>(params, b),
        EntityType::U32 => bench_apply_alignment::<u32>(params, b),
    }
}

benchmark! {
    name = apply;
    ApplyParameters {
        components in &[16384, 32768, 65536],
        mutate_probability in &[0.0, 0.25, 0.5, 0.75, 1.0],
        entity_type in &[EntityType::U128, EntityType::U64, EntityType::U32],
        alignment in &[Alignment::Align16, Alignment::Align32, Alignment::Align64],
        collection_type in &[CollectionType::CopyOnWrite, CollectionType::InsertOptimized, CollectionType::Mutable],
    }
    bench_apply,
}

/////////////////////////////////////////////// main ///////////////////////////////////////////////

statslicer_main! {
    apply
}
