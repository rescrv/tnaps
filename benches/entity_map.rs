use guacamole::combinators::*;
use guacamole::{FromGuacamole, Guacamole};
use statslicer::{benchmark, black_box, statslicer_main, Bencher, Parameter, Parameters};

use tnaps::{Entity, EntityMap, FastEntityMap, VecEntityMap};

const CONSTRUCT_LENS: &[usize] = &[
    1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768, 65536,
];

const MAP_TYPES: &[MapType] = &[MapType::Vec, MapType::Fast];

const ENTITY_TYPES: &[EntityType] = &[EntityType::U128, EntityType::U64, EntityType::U32];

////////////////////////////////////////////// MapType /////////////////////////////////////////////

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum MapType {
    #[default]
    Vec,
    Fast,
}

//////////////////////////////////////////// EntityType ////////////////////////////////////////////

#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
enum EntityType {
    #[default]
    U128,
    U64,
    U32,
}

//////////////////////////////////////// EntityMapParameters ///////////////////////////////////////

#[derive(Debug, Default, Eq, PartialEq)]
struct EntityMapParameters {
    elements: usize,
    map_type: MapType,
    entity_type: EntityType,
}

impl Parameters for EntityMapParameters {
    fn params(&self) -> Vec<(&'static str, Parameter)> {
        let map_type = match self.map_type {
            MapType::Fast => "fast",
            MapType::Vec => "vec",
        };
        let entity_type = match self.entity_type {
            EntityType::U128 => "u128",
            EntityType::U64 => "u64",
            EntityType::U32 => "u32",
        };
        vec![
            ("elements", Parameter::Integer(self.elements as u64)),
            ("map_type", Parameter::Text(map_type.to_string())),
            ("entity_type", Parameter::Text(entity_type.to_string())),
        ]
    }
}

///////////////////////////////////////////// construct ////////////////////////////////////////////

fn bench_construct_entity<E: Entity + FromGuacamole<()>>(
    params: &EntityMapParameters,
    b: &mut Bencher,
) {
    let mut guac = Guacamole::new(b.seed());
    let mut entities = vec![];
    for _ in 0..b.size() {
        let mut ents = to_vec(constant(params.elements), any::<E>)(&mut guac);
        ents.sort();
        ents.dedup();
        entities.push(ents);
    }
    fn construct_fast<E: Entity>(ents: Vec<E>) {
        black_box(FastEntityMap::<E>::from_iter(ents.into_iter()));
    }
    fn construct_vec<E: Entity>(ents: Vec<E>) {
        black_box(VecEntityMap::<E>::from_iter(ents.into_iter()));
    }
    let construct = match params.map_type {
        MapType::Fast => construct_fast,
        MapType::Vec => construct_vec,
    };
    b.run(|| {
        for ents in entities.into_iter() {
            construct(black_box(ents))
        }
    });
}

fn bench_construct(params: &EntityMapParameters, b: &mut Bencher) {
    match &params.entity_type {
        EntityType::U128 => bench_construct_entity::<u128>(params, b),
        EntityType::U64 => bench_construct_entity::<u64>(params, b),
        EntityType::U32 => bench_construct_entity::<u32>(params, b),
    }
}

benchmark! {
    name = entity_map_construct;
    EntityMapParameters {
        elements in CONSTRUCT_LENS,
        map_type in MAP_TYPES,
        entity_type in ENTITY_TYPES,
    }
    bench_construct
}

//////////////////////////////////////////// lower_bound ///////////////////////////////////////////

fn bench_lower_bound_entity<E: Entity + FromGuacamole<()>>(
    params: &EntityMapParameters,
    b: &mut Bencher,
) {
    let mut guac = Guacamole::new(b.seed());
    let mut entities = to_vec(constant(params.elements), any::<E>)(&mut guac);
    entities.sort();
    entities.dedup();
    let queries = to_vec(constant(b.size()), any::<E>)(&mut guac);
    match &params.map_type {
        MapType::Vec => {
            let entities = VecEntityMap::<E>::from_iter(entities);
            b.run(|| {
                for query in queries.into_iter() {
                    black_box(entities.lower_bound(query));
                }
            });
        }
        MapType::Fast => {
            let entities = FastEntityMap::<E>::from_iter(entities);
            b.run(|| {
                for query in queries.into_iter() {
                    black_box(entities.lower_bound(query));
                }
            });
        }
    }
}

fn bench_lower_bound(params: &EntityMapParameters, b: &mut Bencher) {
    match &params.entity_type {
        EntityType::U128 => bench_lower_bound_entity::<u128>(params, b),
        EntityType::U64 => bench_lower_bound_entity::<u64>(params, b),
        EntityType::U32 => bench_lower_bound_entity::<u32>(params, b),
    }
}

benchmark! {
    name = entity_map_lower_bound;
    EntityMapParameters {
        elements in CONSTRUCT_LENS,
        map_type in MAP_TYPES,
        entity_type in ENTITY_TYPES,
    }
    bench_lower_bound
}

///////////////////////////////////////////// offset_of ////////////////////////////////////////////

fn bench_offset_of_entity<E: Entity + FromGuacamole<()>>(
    params: &EntityMapParameters,
    b: &mut Bencher,
) {
    let mut guac = Guacamole::new(b.seed());
    let mut entities = to_vec(constant(params.elements), any::<E>)(&mut guac);
    entities.sort();
    entities.dedup();
    let queries = to_vec(constant(b.size()), any::<E>)(&mut guac);
    match &params.map_type {
        MapType::Vec => {
            let entities = VecEntityMap::<E>::from_iter(entities);
            b.run(|| {
                for query in queries.into_iter() {
                    black_box(entities.offset_of(query));
                }
            });
        }
        MapType::Fast => {
            let entities = FastEntityMap::<E>::from_iter(entities);
            b.run(|| {
                for query in queries.into_iter() {
                    black_box(entities.offset_of(query));
                }
            });
        }
    }
}

fn bench_offset_of(params: &EntityMapParameters, b: &mut Bencher) {
    match &params.entity_type {
        EntityType::U128 => bench_offset_of_entity::<u128>(params, b),
        EntityType::U64 => bench_offset_of_entity::<u64>(params, b),
        EntityType::U32 => bench_offset_of_entity::<u32>(params, b),
    }
}

benchmark! {
    name = entity_map_offset_of;
    EntityMapParameters {
        elements in CONSTRUCT_LENS,
        map_type in MAP_TYPES,
        entity_type in ENTITY_TYPES,
    }
    bench_offset_of
}

statslicer_main! {
    entity_map_construct,
    entity_map_lower_bound,
    entity_map_offset_of,
}
