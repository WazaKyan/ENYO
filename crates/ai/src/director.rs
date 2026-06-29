//! Directeur temps réel — **version INTENTION** (durcie après audit cruel).
//!
//! Problème du temps réel : quand DeepSeek répond (~10-30 s), des dizaines de
//! mois ont passé → une commande *précise* viserait un état périmé, donc
//! visible. Solution : le LLM (ou la baseline déterministe) pose une **intention
//! durable** (posture + intensité + durée + cible), et CE module la **résout
//! chaque tick contre l'état COURANT** en leviers concrets.
//!
//! Invisibilité (corrections d'audit H1/H2) : la cadence est **apériodique**,
//! les montants **variés**, les cibles tirées parmi un **top-K** (jamais l'argmax
//! strict) en **évitant de répéter** la même case. Tout le hasard vient d'un
//! **jitter PUR** (SplitMix sur `world.turn`) — il **ne touche JAMAIS `world.rng`**
//! (le sim ne tire qu'un `next_u64()` par Step ; un tirage de plus casserait la
//! météo et les golden). Équité (M4) : une stance qui n'est plus justifiée est
//! abandonnée, et un **Budget d'Équité** plafonne l'acharnement.
//!
//! Déterminisme : l'intention et l'état du Directeur sont **LIVE only** (jamais
//! dans `World`/`.rec`). Seuls les leviers concrets émis sont enregistrés → le
//! rejeu les rejoue tels quels, sans rappeler le LLM ni ce résolveur.

use proto::Command;
use sim::World;

use crate::{assess, DOMINANCE_BLIGHT, DOMINANCE_PRESSURE};

/// Posture narrative du Directeur (le « ton » qu'il imprime à la partie).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stance {
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
    /// Durée **relative** (mois) — ré-ancrée à l'application (corrige M2 : une
    /// intention LLM ne doit pas être « morte à l'arrivée »).
    pub duration: u64,
    /// Mois de jeu au-delà duquel l'intention expire (= `world.turn` au moment où
    /// elle est posée `+ duration`).
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
            duration: 6,
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
        if d.dominance > DOMINANCE_PRESSURE {
            let intensity = ((d.dominance * 200.0) as i64).clamp(20, 100) as u32;
            Self {
                stance: Stance::Pressure,
                intensity,
                duration: 12,
                until_turn: world.turn + 12,
                focus: d.strongest_rival,
                public_cause: "tensions régionales et intempéries".into(),
                hidden_intent: "freiner le joueur dominant".into(),
            }
        } else if d.struggling {
            Self {
                stance: Stance::Relief,
                intensity: 60,
                duration: 12,
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

    /// Ré-ancre la fin de l'intention sur le tour COURANT (à l'application).
    pub fn anchor(&mut self, now: u64) {
        self.until_turn = now + self.duration.max(ACT_PERIOD_MAX);
    }
}

/// Bornes de la cadence apériodique (mois entre deux interventions).
const ACT_PERIOD_MIN: u64 = 2;
const ACT_PERIOD_MAX: u64 = 5;
/// Budget d'Équité : plafonne l'acharnement cumulé (anti-persécution, M4).
const EQUITY_MAX: i32 = 120;

/// Contrôleur du Directeur — **état LIVE** (jamais sérialisé, jamais rejoué).
pub struct Director {
    intent: Intent,
    next_act: u64,
    last_target: Option<(u32, u32)>,
    /// Budget d'équité : se régénère lentement, se dépense en Pression.
    pressure_budget: i32,
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
            next_act: 0,
            last_target: None,
            pressure_budget: EQUITY_MAX,
        }
    }

    /// Pose une nouvelle intention (depuis le LLM) — ré-ancrée sur `now`.
    pub fn set_intent(&mut self, mut intent: Intent, now: u64) {
        intent.anchor(now);
        self.intent = intent;
    }

    /// L'intention courante (lecture seule, pour le HUD d'audit).
    pub fn intent(&self) -> &Intent {
        &self.intent
    }

    /// Résout l'intention COURANTE contre l'état COURANT → leviers concrets.
    pub fn resolve_tick(&mut self, world: &World, player: u16) -> Vec<Command> {
        // Régénération lente du Budget d'Équité (anti-acharnement).
        if self.pressure_budget < EQUITY_MAX {
            self.pressure_budget += 1;
        }
        // Renouvellement de l'intention expirée par la baseline déterministe.
        if world.turn >= self.intent.until_turn {
            self.intent = Intent::baseline(world, player);
        }
        // Cadence APÉRIODIQUE (anti-métronome H2).
        if self.next_act == 0 {
            self.next_act = world.turn + act_gap(world.turn);
        }
        if world.turn < self.next_act {
            return Vec::new();
        }
        self.next_act = world.turn + act_gap(world.turn);

        let d = assess(world, player);
        if d.nations < 2 {
            return Vec::new();
        }

        // GARDE D'ÉQUITÉ (M4) : abandonner une stance qui n'est plus justifiée.
        let stance = match self.intent.stance {
            Stance::Pressure if d.dominance <= DOMINANCE_PRESSURE => Stance::Neutral,
            Stance::Relief if !d.struggling => Stance::Neutral,
            s => s,
        };
        let i = self.intent.intensity;
        let t = world.turn;
        let mut cmds = Vec::new();

        match stance {
            Stance::Neutral => {}
            Stance::Pressure => {
                if self.pressure_budget <= 0 {
                    return Vec::new(); // acharnement plafonné
                }
                if let Some(r) = valid_focus(world, self.intent.focus, player).or(d.strongest_rival) {
                    // Grief doux : ne franchit pas vite le seuil de guerre (M4).
                    let amount = vary(i / 18, t ^ 0xA1, 1, 6);
                    cmds.push(Command::DirectorGrievance { from: r, to: player, amount });
                    self.pressure_budget -= amount as i32;
                }
                // Calamité seulement si domination forte ET budget restant.
                if d.dominance > DOMINANCE_BLIGHT && self.pressure_budget > 0 {
                    let cands = top_pop_tiles(world, player, 8);
                    if let Some((x, y)) = pick(&cands, t ^ 0xB2, self.last_target) {
                        let amount = vary(i / 5, t ^ 0xC3, 1, 30);
                        cmds.push(Command::DirectorBlight { x, y, amount });
                        self.last_target = Some((x, y));
                        self.pressure_budget -= amount as i32;
                    }
                }
            }
            Stance::Relief => {
                // Le « salut » va sur une région peuplée (plausible, varié, jamais
                // le coin dégénéré) — corrige H1.
                let cands = top_pop_tiles(world, player, 8);
                if let Some((x, y)) = pick(&cands, t ^ 0xD4, self.last_target) {
                    let amount = vary(i / 2, t ^ 0xE5, 1, 40);
                    cmds.push(Command::DirectorWindfall { x, y, amount });
                    self.last_target = Some((x, y));
                }
            }
            Stance::ElevateRival => {
                if let Some(r) = valid_focus(world, self.intent.focus, player).or(d.strongest_rival) {
                    let cands = top_pop_tiles(world, r, 6);
                    if let Some((x, y)) = pick(&cands, t ^ 0xF6, None) {
                        let amount = vary(i / 3, t ^ 0x17, 1, 40);
                        cmds.push(Command::DirectorWindfall { x, y, amount });
                    }
                    let amount = vary(i / 20, t ^ 0x28, 1, 6);
                    cmds.push(Command::DirectorGrievance { from: r, to: player, amount });
                }
            }
        }
        cmds
    }
}

/// SplitMix64 **pur** — jitter du Directeur. NE touche PAS `world.rng`.
fn jitter(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Intervalle apériodique (mois) avant la prochaine intervention.
fn act_gap(turn: u64) -> u64 {
    ACT_PERIOD_MIN + jitter(turn ^ 0x9151) % (ACT_PERIOD_MAX - ACT_PERIOD_MIN + 1)
}

/// Montant varié (±20 %) autour de `base`, borné [lo, hi].
fn vary(base: u32, seed: u64, lo: u32, hi: u32) -> u32 {
    let factor = 80 + jitter(seed) % 41; // 80..=120 %
    ((base.max(1) as u64 * factor) / 100).clamp(lo as u64, hi as u64) as u32
}

/// `focus` n'est gardé que s'il désigne une nation EXISTANTE et différente du
/// joueur (corrige M1 : pas de grief fantôme / réflexif / inversé).
fn valid_focus(world: &World, focus: Option<u16>, player: u16) -> Option<u16> {
    focus.filter(|&r| r != player && world.nation(r).is_some())
}

/// Top-K cases les plus peuplées d'une nation (insertion bornée, 1 passe, déterministe).
fn top_pop_tiles(world: &World, owner: u16, k: usize) -> Vec<(u32, u32)> {
    let mut top: Vec<(f32, (u32, u32))> = Vec::with_capacity(k + 1);
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(owner) {
            continue;
        }
        let c = (idx as u32 % world.width, idx as u32 / world.width);
        let p = t.population;
        if top.len() < k || p > top[top.len() - 1].0 {
            let pos = top.iter().position(|(q, _)| p > *q).unwrap_or(top.len());
            top.insert(pos, (p, c));
            if top.len() > k {
                top.pop();
            }
        }
    }
    top.into_iter().map(|(_, c)| c).collect()
}

/// Tire une cible parmi les candidats via jitter, en évitant la dernière touchée.
fn pick(cands: &[(u32, u32)], seed: u64, last: Option<(u32, u32)>) -> Option<(u32, u32)> {
    if cands.is_empty() {
        return None;
    }
    let mut i = (jitter(seed) % cands.len() as u64) as usize;
    if cands.len() > 1 && Some(cands[i]) == last {
        i = (i + 1) % cands.len();
    }
    Some(cands[i])
}
