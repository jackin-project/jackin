// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

/// Curated wordlist for assigning unique, human-memorable codenames to tabs.
///
/// Three semantic families — animals, landforms, weather/celestial — chosen
/// for short length, unambiguous pronunciation, and easy orientation. A total
/// of ~150 words gives a pool far larger than any realistic tab fleet.
///
/// Assignment cycles from a random offset seeded once at daemon start.
/// Codenames are **never reused** within a container lifetime: once a word is
/// assigned to any tab (even a now-closed one), it moves to the retired set and
/// is not picked again. When the pool is exhausted the fallback appends `-2`,
/// `-3`, … to previously-used bare words until a unique name is found.
pub static WORDLIST: &[&str] = &[
    // Animals
    "badger",
    "crane",
    "falcon",
    "gecko",
    "heron",
    "ibex",
    "jackal",
    "kite",
    "lynx",
    "mink",
    "newt",
    "otter",
    "puma",
    "quail",
    "raven",
    "stoat",
    "teal",
    "vole",
    "wren",
    "yak",
    "adder",
    "bison",
    "cobra",
    "dingo",
    "egret",
    "finch",
    "grebe",
    "hyena",
    "impala",
    "jaguar",
    "kudu",
    "lemur",
    "moose",
    "narwhal",
    "ocelot",
    "puffin",
    "quokka",
    "robin",
    "skunk",
    "tapir",
    "urubu",
    "viper",
    "wombat",
    "xerus",
    "zebu",
    "axolotl",
    "boar",
    "coyote",
    "dhole",
    "ermine",
    "ferret",
    "goshawk",
    "harrier",
    "iguazu",
    "junco",
    "kestrel",
    "limpet",
    "marten",
    "nutria",
    "osprey",
    "petrel",
    // Landforms
    "arch",
    "bay",
    "bluff",
    "cape",
    "cliff",
    "crag",
    "delta",
    "dune",
    "fjord",
    "gorge",
    "gulch",
    "inlet",
    "isle",
    "knoll",
    "ledge",
    "mesa",
    "moor",
    "peak",
    "reef",
    "ridge",
    "scarp",
    "shelf",
    "shoal",
    "sill",
    "spit",
    "spur",
    "steppe",
    "swale",
    "talus",
    "tor",
    "vale",
    "atoll",
    "butte",
    "cirque",
    "draw",
    "flats",
    "gully",
    "heath",
    "karst",
    "loch",
    "marsh",
    "notch",
    "overhang",
    "plain",
    "quarry",
    "ravine",
    "scree",
    "tarn",
    "vent",
    "wash",
    "xenolith",
    "yardang",
    // Weather and celestial
    "aurora",
    "cirrus",
    "frost",
    "gale",
    "haze",
    "mist",
    "nimbus",
    "squall",
    "storm",
    "surge",
    "tide",
    "veil",
    "zephyr",
    "anvil",
    "bora",
    "cloud",
    "drizzle",
    "eddy",
    "flurry",
    "gloom",
    "hail",
    "inflow",
    "jetstream",
    "katabatic",
    "lull",
    "mirage",
    "nebula",
    "overcast",
    "plume",
    "quasar",
    "rime",
    "sleet",
];

/// Pick the next codename that appears in neither `live` nor `retired`.
///
/// Cycles through `WORDLIST` starting at `offset % WORDLIST.len()`.
/// Falls back to `<word>-N` (N ≥ 2) appended to retired words when the
/// entire bare-word pool is exhausted — ensures uniqueness without panic.
pub fn pick_codename(
    live: &std::collections::HashSet<String>,
    retired: &std::collections::HashSet<String>,
    offset: usize,
) -> String {
    let len = WORDLIST.len();
    // First pass: find an unused bare word.
    for i in 0..len {
        let word = WORDLIST[(offset + i) % len];
        if !live.contains(word) && !retired.contains(word) {
            return word.to_owned();
        }
    }
    // Fallback: extend a retired word with a numeric suffix until unique.
    let base = WORDLIST[offset % len];
    for n in 2u32.. {
        let candidate = format!("{base}-{n}");
        if !live.contains(&candidate) && !retired.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("infinite suffix loop always terminates")
}

#[cfg(test)]
mod tests;
