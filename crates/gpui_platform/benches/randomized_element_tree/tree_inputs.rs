//! The tree families the randomized element tree benchmarks sample, shared so that an
//! input names the same tree in the timing and the memory benchmark.

use std::fmt;

use gpui::randomized_element_tree::{
    RandomizedElementTreeBounds, RandomizedElementTreeConfig, RandomizedElementTreeTopology,
};

/// How many seeds each family is sampled at. Each seed is reported on its own; the
/// family's average is read across them.
pub const SEEDS_PER_FAMILY: u64 = 6;

/// A named family of trees, and one seed's draw from it. `seed` counts within the family;
/// the config's seed is offset per family so families with overlapping bounds do not
/// draw the same trees.
#[derive(Clone)]
pub struct TreeInput {
    pub family: &'static str,
    pub seed: u64,
    pub config: RandomizedElementTreeConfig,
}

impl fmt::Display for TreeInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let topology = match self.config.topology() {
            RandomizedElementTreeTopology::Wide => "wide",
            RandomizedElementTreeTopology::Deep => "deep",
            RandomizedElementTreeTopology::Mixed => "mixed",
            RandomizedElementTreeTopology::Narrow => "narrow",
        };
        write!(
            formatter,
            "{}-s{}-{topology}-e{}-ent{}",
            self.family,
            self.seed,
            self.config.element_count(),
            (self.config.entity_density() * 100.0).round() as usize
        )
    }
}

/// Tall trees recurse once per element through layout and paint; without gpui's `stacker`
/// feature a chain of 384 draws and one of 512 overflows a 2 MB stack, so tall families
/// stop well short of that. Real UI is not hundreds of levels deep either.
const TALL_MAX_ELEMENTS: usize = 256;

/// Entities are how GPUI structures a UI, so every family has at least this share of its
/// elements backed by one; a tree with none is a single view, which no renderer can
/// partially reuse and which real UI never is.
const MIN_ENTITY_DENSITY: f64 = 0.05;

/// The families every benchmark samples. Each is a question: what does this class of
/// change cost on a tree like *this*?
fn families() -> Vec<(&'static str, RandomizedElementTreeBounds)> {
    let handlers = 0.1..=0.4;
    vec![
        (
            "any",
            RandomizedElementTreeBounds::new(32..=TALL_MAX_ELEMENTS)
                .with_entity_density(MIN_ENTITY_DENSITY..=0.5)
                .with_handler_density(handlers.clone()),
        ),
        (
            "wide",
            RandomizedElementTreeBounds::new(128..=2048)
                .with_topologies([RandomizedElementTreeTopology::Wide])
                .with_entity_density(MIN_ENTITY_DENSITY..=0.25)
                .with_handler_density(handlers.clone()),
        ),
        (
            "tall",
            RandomizedElementTreeBounds::new(32..=TALL_MAX_ELEMENTS)
                .with_topologies(
                    RandomizedElementTreeTopology::ALL
                        .into_iter()
                        .filter(|topology| topology.is_tall()),
                )
                .with_entity_density(MIN_ENTITY_DENSITY..=0.25)
                .with_handler_density(handlers.clone()),
        ),
        (
            "mixed",
            RandomizedElementTreeBounds::new(128..=1024)
                .with_topologies([RandomizedElementTreeTopology::Mixed])
                .with_entity_density(MIN_ENTITY_DENSITY..=0.25)
                .with_handler_density(handlers.clone()),
        ),
        (
            "dense-entities",
            RandomizedElementTreeBounds::new(128..=1024)
                .with_topologies([
                    RandomizedElementTreeTopology::Wide,
                    RandomizedElementTreeTopology::Mixed,
                ])
                .with_entity_density(0.75..=1.0)
                .with_handler_density(handlers),
        ),
    ]
}

pub fn inputs() -> Vec<TreeInput> {
    families()
        .into_iter()
        .enumerate()
        .flat_map(|(family_index, (family, bounds))| {
            (0..SEEDS_PER_FAMILY).map(move |seed| TreeInput {
                family,
                seed,
                config: bounds.sample((family_index as u64) << 32 | seed),
            })
        })
        .collect()
}
