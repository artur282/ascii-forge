//! Internal deterministic randomness for effects.
//!
//! SplitMix64 is implemented here instead of pulling in an RNG crate: effect
//! output must not depend on dependency versions, wall clocks or process
//! entropy. Every random value is derived from the scene seed, the effect seed
//! and a stable hash of the effect id.

const GOLDEN_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const FNV_OFFSET: u64 = 0xCBF2_9CE4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01B3;

/// Deterministic SplitMix64 stream.
pub(crate) struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GOLDEN_GAMMA);
        mix64(self.state)
    }

    /// Uniform value in `0..upper`; zero when the range is empty.
    pub(crate) fn bounded(&mut self, upper: u64) -> u64 {
        if upper == 0 {
            0
        } else {
            self.next_u64() % upper
        }
    }

    pub(crate) fn coin(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

/// SplitMix64 finalizer, also used to derive independent sub-seeds.
pub(crate) fn mix64(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

/// Combine the scene seed, the effect seed (when present) and the effect id.
pub(crate) fn combine(scene_seed: u64, effect_seed: Option<u64>, effect_id: &str) -> u64 {
    let mut state = scene_seed ^ id_hash(effect_id).rotate_left(32);
    if let Some(seed) = effect_seed {
        state ^= mix64(seed);
    }
    mix64(state)
}

/// Stable per-cell hash used by per-cell reveal effects.
pub(crate) fn cell_hash(seed: u64, index: usize) -> u64 {
    mix64(seed ^ (index as u64).wrapping_mul(GOLDEN_GAMMA))
}

/// FNV-1a over the id bytes: stable across platforms and Rust versions.
fn id_hash(text: &str) -> u64 {
    let mut hash = FNV_OFFSET;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}
