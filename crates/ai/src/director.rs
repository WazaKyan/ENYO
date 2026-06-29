//! Directeur temps réel — **version INTENTION**.
//!
//! Problème du temps réel : quand DeepSeek répond (~10-30 s), des dizaines de
//! mois ont passé → une commande *précise* (« calamité sur la case (412,233) »)
//! viserait un état périmé, donc **visible**. Solution : le LLM (ou la baseline
//! déterministe) pose une **intention durable** (posture + intensité + durée +
//! cible), et CE module la **résout chaque tick contre l'état COURANT** en
//! leviers concrets (`DirectorGrievance`/`Blight`/`Windfall`). L'effet colle
//! donc toujours à une cause organique *actuelle* → plus invisible.
//!
//! Déterminisme : l'intention est un état **LIVE only** (jamais dans `World` ni
//! dans le `.rec`). Seuls les **leviers concrets** émis sont enregistrés → le
//! rejeu les rejoue tels quels, sans jamais rappeler le LLM ni ce résolveur.

use proto::Command;
use sim::World;

use crate::assess;

/// Posture narrative du Directeur (le « ton » qu'il imprime à la partie).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stance {
    /// Rien à mettre en scène.
    Neutral,
    /// Le joueur domine : on l'enserre (griefs des rivaux, calamités « naturelles »).
    Pressure,
    /// Le joueur souffre injustement : répit discret.
    Relief,
    /// Faire monter un rival pour créer du drame.
    ElevateRival,
}

/// Intention durable du Directeur, posée par le LLM ou la baseline déterministe.
#[derive(Clone, Debug)]
pub struct Intent {
    pub stance: Stance,
    /// 0..=100. Échelle les montants des leviers et déclenche les leviers lourds.
    pub intensity: u32,
    /// Mois de jeu (`world.turn`) au-delà duquel l'intention expire (renouvelée).
    pub until_turn: u64,
    /// Nation visée (rival) pour `Pressure`/`ElevateRival`.
    pub focus: Option<u16>,
    /// Cause organique affichable (vue du joueur) — pour l'audit.
    pub public_cause: String,
    /// Vraie raison (jamais montrée) — pour l'audit / le log.
    pub hidden_intent: String,
}

impl Intent {
    pub fn neutral() -> Self {
        Self {
            stance: Stance::Neutral,
            intensity: 0,
            until_turn: 0,
            focus: None,
            public_cause: String::new(),
            hidden_intent: String::new(),
        }
    }

    /// Intention de repli **déterministe**, dérivée de l'Indice de Drame.
    /// (C'est la baseline shippable : la partie tient sans LLM.)
    pub fn baseline(world: &World, player: u16) -> Self {
        let d = assess(world, player);
        if d.nations < 2 {
            return Self::neutral();
        }
        if d.dominance > 0.10 {
            let intensity = ((d.dominance * 200.0) as i64).clamp(20, 100) as u32;
            Self {
                stance: Stance::Pressure,
                intensity,
                until_turn: world.turn + 12,
                focus: d.strongest_rival,
                public_cause: "tensions régionales et intempéries".into(),
                hidden_intent: "freiner le joueur dominant".into(),
            }
        } else if d.struggling {
            Self {
                stance: Stance::Relief,
                intensity: 50,
                until_turn: world.turn + 12,
                focus: None,
                public_cause: "saison clémente".into(),
                hidden_intent: "répit pour un joueur en difficulté méritée".into(),
            }
        } else {
            let mut n = Self::neutral();
            n.until_turn = world.turn + 6;
            n
        }
    }
}

/// Cadence minimale (en mois de jeu) entre deux interventions concrètes :
/// borne le débit de leviers à l'échelle du JEU, pas du mur (anti-spam à x4/Max).
const ACT_PERIOD: u64 = 3;

/// Contrôleur du Directeur — **état LIVE** (jamais sérialisé, jamais rejoué).
pub struct Director {
    intent: Intent,
    last_act: u64,
}

impl Default for Director {
    fn default() -> Self {
        Self::new()
    }
}

impl Director {
    pub fn new() -> Self {
        Self {
            intent: Intent::neutral(),
            last_act: 0,
        }
    }

    /// Pose une nouvelle intention (depuis le LLM, généralement).
    pub fn set_intent(&mut self, intent: Intent) {
        self.intent = intent;
    }

    /// L'intention courante (lecture seule, pour le HUD d'audit).
    pub fn intent(&self) -> &Intent {
        &self.intent
    }

    /// Résout l'intention COURANTE contre l'état COURANT → leviers concrets.
    /// Renouvelle l'intention par la baseline déterministe si elle a expiré.
    pub fn resolve_tick(&mut self, world: &World, player: u16) -> Vec<Command> {
        if world.turn >= self.intent.until_turn {
            self.intent = Intent::baseline(world, player);
        }
        // Intervenir au plus tous les ACT_PERIOD mois (cadence de jeu).
        if world.turn < self.last_act.saturating_add(ACT_PERIOD) && self.last_act != 0 {
            return Vec::new();
        }
        let d = assess(world, player);
        if d.nations < 2 {
            return Vec::new();
        }
        let i = self.intent.intensity;
        let mut cmds = Vec::new();
        match self.intent.stance {
            Stance::Neutral => {}
            Stance::Pressure => {
                if let Some(r) = self.intent.focus.or(d.strongest_rival) {
                    cmds.push(Command::DirectorGrievance {
                        from: r,
                        to: player,
                        amount: (i / 15).clamp(1, 10),
                    });
                }
                if i >= 60 {
                    if let Some((x, y)) = d.player_best_tile {
                        cmds.push(Command::DirectorBlight {
                            x,
                            y,
                            amount: (i / 4).clamp(1, 30),
                        });
                    }
                }
            }
            Stance::Relief => {
                if let Some((x, y)) = d.player_worst_tile {
                    cmds.push(Command::DirectorWindfall {
                        x,
                        y,
                        amount: (i / 3).clamp(1, 40),
                    });
                }
            }
            Stance::ElevateRival => {
                if let Some(r) = self.intent.focus.or(d.strongest_rival) {
                    if let Some((x, y)) = rival_best_tile(world, r) {
                        cmds.push(Command::DirectorWindfall {
                            x,
                            y,
                            amount: (i / 3).clamp(1, 40),
                        });
                    }
                    cmds.push(Command::DirectorGrievance {
                        from: r,
                        to: player,
                        amount: (i / 20).clamp(1, 8),
                    });
                }
            }
        }
        if !cmds.is_empty() {
            self.last_act = world.turn.max(1);
        }
        cmds
    }
}

/// Case la plus peuplée d'une nation rivale (cible d'un coup de pouce organique).
fn rival_best_tile(world: &World, rival: u16) -> Option<(u32, u32)> {
    let mut chosen = None;
    let mut best = -1.0f32;
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(rival) {
            continue;
        }
        if t.population > best {
            best = t.population;
            chosen = Some((idx as u32 % world.width, idx as u32 / world.width));
        }
    }
    chosen
}
