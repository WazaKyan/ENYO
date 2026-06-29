//! Le « langage » du jeu : commandes (entrées) et événements (sorties).
//!
//! Tout changement d'état passe par une [`Command`] qui produit des [`Event`].
//! Le journal JSONL suffit à auditer et rejouer une partie (event-sourcing).
//! Chaque événement de tour porte un `checksum` déterministe ; les commandes
//! rejetées sont elles aussi loguées (audit complet).

use serde::{Deserialize, Serialize};

/// Vocation d'une case (système S8 — économie interne). Une case n'en porte qu'une.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Building {
    /// Ville : **produit de la population** (croissance vers la capacité du terrain),
    /// **consomme de la nourriture**. C'est la « case d'habitation » à laquelle les
    /// autres bâtiments se connectent. Coûte de l'habitation + de l'argent à fonder.
    City,
    /// Produit des matériaux (∝ stats de case × pop connectée) ; pollue (dévastation).
    Industry,
    /// Transforme les matériaux en argent + habitation + croissance.
    Commerce,
    /// Relie les cases en réseau (routes) — pas de production.
    Infrastructure,
    /// Génère de la science (exige habitation + commerce connectés).
    Education,
    /// Génère des soldats (force) ; entretien mensuel.
    Military,
    /// Produit de la nourriture (rendement ∝ terrain : humidité, température…).
    Farm,
}

/// Type d'unité militaire (S5). Débloqué par la branche **Fer** ; chaque type a
/// ses stats (PV, dégâts, portée, mouvement) et ses affinités de terrain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum UnitKind {
    /// Infanterie : polyvalente, robuste, corps à corps (aucun malus de terrain).
    Infantry,
    /// Archers : attaque à distance (portée 2), fragiles, **malus en forêt**.
    Archer,
    /// Cavalerie : rapide et puissante, **malus en terrain accidenté/forêt**.
    Cavalry,
}

/// Une action demandée à la simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Avance la partie d'un tour (un mois de jeu).
    Step,
    /// Implante une population de départ d'une nation sur une case de terre.
    Settle {
        x: u32,
        y: u32,
        nation: u16,
        population: u32,
    },
    /// Essaimage : déplace la moitié de la population d'une case vers une cible
    /// atteignable (selon la portée technologique). Source ≥ 1000 requis.
    Swarm {
        from_x: u32,
        from_y: u32,
        to_x: u32,
        to_y: u32,
    },
    /// Investit le savoir d'une nation dans une branche de l'arbre de tech (0..4).
    Research { nation: u16, branch: u8 },
    /// Bâtit un bâtiment (S8) sur une case possédée et vide, si la nation paie le coût.
    Build {
        x: u32,
        y: u32,
        nation: u16,
        building: Building,
    },
    /// Démolit le bâtiment d'une case possédée (pour reconstruire autre chose).
    /// Rembourse la moitié du coût × l'état (1 − dévastation) de la case.
    Demolish { x: u32, y: u32, nation: u16 },
    /// Recrute une **unité** (S5) sur une case possédée portant une **caserne**.
    /// Coûte de l'argent + de la force (générée par la caserne) ; type tech-gaté.
    CreateUnit {
        x: u32,
        y: u32,
        nation: u16,
        kind: UnitKind,
    },
    /// Déplace une unité vers une case atteignable dans ses points de mouvement
    /// (coût terrain + intempéries). L'unité est désignée par son id.
    MoveUnit { unit: u32, to_x: u32, to_y: u32 },
    /// Attaque avec une unité une case à portée contenant une unité ennemie
    /// (combat avec bonus de défense du terrain + malus d'attaque selon le type).
    AttackUnit { unit: u32, x: u32, y: u32 },
    /// Dote une nation en ressources (genèse : coup de pouce aux IA). Commande
    /// **enregistrée** → le rejeu reproduit la dotation (déterminisme préservé).
    Endow {
        nation: u16,
        money: i64,
        materials: i64,
        influence: i64,
        housing: i64,
        food: i64,
    },
    /// Déclare la guerre à une autre nation.
    DeclareWar { nation: u16, target: u16 },
    /// Fait la paix avec une autre nation.
    MakePeace { nation: u16, target: u16 },
    /// [Directeur] Attise un grief (biais d'opinion, S6) pour fabriquer des coalitions.
    DirectorGrievance { from: u16, to: u16, amount: u32 },
    /// [Directeur] Calamité localisée (sécheresse/fléau) biaisant une case (S1).
    DirectorBlight { x: u32, y: u32, amount: u32 },
    /// [Directeur] Aubaine localisée (bonne récolte) — salut discret (S1).
    DirectorWindfall { x: u32, y: u32, amount: u32 },
}

/// Un fait advenu dans la simulation, produit par l'application d'une [`Command`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Event {
    /// Le monde a été généré. Résumé + `checksum` (audit de reproductibilité).
    WorldGenerated {
        seed: u64,
        width: u32,
        height: u32,
        land_tiles: u32,
        ocean_tiles: u32,
        checksum: u64,
    },
    /// Un tour (mois) a été résolu. Agrégats + `checksum` (audit de déterminisme).
    TurnResolved {
        turn: u64,
        month: u8,
        avg_temperature: f32,
        avg_vegetation: f32,
        checksum: u64,
    },
    /// Une population de départ a été implantée.
    Settled {
        nation: u16,
        x: u32,
        y: u32,
        population: u32,
    },
    /// Un essaimage a eu lieu (population déplacée vers la cible).
    Swarmed {
        nation: u16,
        from_x: u32,
        from_y: u32,
        to_x: u32,
        to_y: u32,
        moved: f32,
    },
    /// Une technologie a été débloquée (nouveau palier d'une branche).
    Researched { nation: u16, branch: u8, tier: u8 },
    /// Un bâtiment a été construit (S8 — économie interne).
    Built {
        x: u32,
        y: u32,
        nation: u16,
        building: Building,
    },
    /// Un bâtiment a été démoli (remboursement partiel selon l'état de la case).
    Demolished {
        x: u32,
        y: u32,
        building: Building,
        refund: i64,
    },
    /// Une unité a été recrutée (S5).
    UnitCreated {
        unit: u32,
        nation: u16,
        kind: UnitKind,
        x: u32,
        y: u32,
    },
    /// Une unité s'est déplacée (coût en points de mouvement consommé).
    UnitMoved {
        unit: u32,
        to_x: u32,
        to_y: u32,
        cost: u32,
    },
    /// Une unité en a attaqué une autre. `killed` = la défenseuse est détruite.
    UnitAttacked {
        attacker: u32,
        defender: u32,
        x: u32,
        y: u32,
        damage: i32,
        counter: i32,
        killed: bool,
    },
    /// Une unité a été détruite (PV ≤ 0) — retirée du monde.
    UnitDestroyed { unit: u32, x: u32, y: u32 },
    /// Une nation a **capitulé** : `winner` annexe les `tiles` cases qu'il occupait
    /// (valant `score` points de victoire) et la paix est imposée (S5/S6).
    Capitulation {
        winner: u16,
        loser: u16,
        tiles: u32,
        score: i64,
    },
    /// Une nation a été dotée en ressources (genèse).
    Endowed { nation: u16 },
    /// Une guerre a été déclarée.
    WarDeclared { nation: u16, target: u16 },
    /// La paix a été conclue.
    PeaceMade { nation: u16, target: u16 },
    /// Un grief est né (casus belli) — ex. essaimage sur une case ennemie.
    GrievanceRaised { from: u16, to: u16, x: u32, y: u32 },
    /// [Directeur] Opinion attisée (biais invisible).
    OpinionNudged { from: u16, to: u16, amount: f32 },
    /// [Directeur] Une calamité a frappé une case.
    Blighted { x: u32, y: u32, amount: f32 },
    /// [Directeur] Une aubaine a béni une case.
    Windfall { x: u32, y: u32, amount: f32 },
    /// Une commande a été rejetée — logué pour l'audit (rien n'est silencieux).
    CommandRejected { reason: String },
}
