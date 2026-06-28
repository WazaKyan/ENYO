//! RNG déterministe (SplitMix64).
//!
//! Algorithme fixe, sans dépendance : une graine donnée produit toujours la même
//! suite, sur toutes les versions. C'est la pierre angulaire du déterminisme
//! (voir le contrat dans `CLAUDE.md`).

/// Générateur pseudo-aléatoire déterministe.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Crée un RNG à partir d'une graine.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Tire le prochain `u64`. Déterministe pour une graine donnée.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seed_diverges() {
        let mut a = Rng::new(1);
        let mut b = Rng::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }
}
