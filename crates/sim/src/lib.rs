//! Cœur de simulation d'ENYO : état du monde et application des commandes.
//!
//! Pur, déterministe, sans I/O ni rendu (headless-first). L'unique façon de
//! modifier l'état est [`World::apply`], qui transforme une [`Command`] en
//! [`Event`]s (event-sourcing).

pub mod rng;

use proto::{Command, Event};
use rng::Rng;

/// L'état complet de la partie.
///
/// Entièrement reconstructible à partir d'une graine et d'une suite de commandes,
/// ce qui garantit le replay déterministe.
#[derive(Debug, Clone)]
pub struct World {
    /// Numéro du tour courant (0 = monde initial).
    pub turn: u64,
    /// RNG déterministe de la partie.
    rng: Rng,
}

impl World {
    /// Crée un monde neuf à partir d'une graine.
    pub fn new(seed: u64) -> Self {
        Self {
            turn: 0,
            rng: Rng::new(seed),
        }
    }

    /// Applique une commande, met à jour l'état, et renvoie les événements produits.
    ///
    /// C'est l'UNIQUE porte d'entrée pour modifier le monde.
    pub fn apply(&mut self, command: Command) -> Vec<Event> {
        match command {
            Command::Step => {
                self.turn += 1;
                let roll = self.rng.next_u64();
                tracing::trace!(turn = self.turn, roll, "tour avancé");
                vec![Event::TurnAdvanced {
                    turn: self.turn,
                    roll,
                }]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_advances_turn() {
        let mut world = World::new(7);
        let events = world.apply(Command::Step);
        assert_eq!(world.turn, 1);
        assert_eq!(events.len(), 1);
    }
}
