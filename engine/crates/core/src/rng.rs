//! Seeded deterministic RNG for the sim, matching the PS2 SCE libc `rand()`
//! both games call — traced from the decomp (identical bodies):
//!   BF1 FUN_0042ab40 (SWBFRONT.ELF), BF2 FUN_005220a0 (SWBFRNT2.ELF):
//!     state = state * 0x41C64E6D + 0x3039;
//!     return state & 0x7FFFFFFF;   // 31-bit output, NO >>16 shift
//!   srand (fn immediately preceding each) stores the seed verbatim.
//! This and [`crate::lcg::GameLcg`] are separate seeded sim streams. Never
//! use wall-clock or OS RNG in sim logic (PLAN.md determinism spine).

#[derive(Clone, Debug)]
pub struct SimRng {
    state: u32,
}

impl SimRng {
    /// `srand`: seed stored verbatim (RE: fn preceding rand in both corpora).
    pub fn new(seed: u32) -> Self {
        Self { state: seed }
    }

    /// SCE `rand()`: 31-bit output.
    pub fn next_rand(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(0x41C6_4E6D).wrapping_add(0x3039);
        self.state & 0x7FFF_FFFF
    }

    pub fn state(&self) -> u32 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sce_rand_reference_sequence() {
        // Tier-1 reference trace: LCG per BF1 FUN_0042ab40 / BF2 FUN_005220a0.
        let mut r = SimRng::new(1);
        let seq: Vec<u32> = (0..5).map(|_| r.next_rand()).collect();
        assert_eq!(
            seq,
            vec![1103527590, 377401575, 662824084, 1147902781, 2035015474]
        );
        let mut r = SimRng::new(0xDEAD_BEEF);
        let seq: Vec<u32> = (0..3).map(|_| r.next_rand()).collect();
        assert_eq!(seq, vec![469847548, 523840645, 1791404762]);
    }
}
