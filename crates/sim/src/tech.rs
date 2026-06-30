//! Arbre de recherche (S3, refonte EU5) : un **graphe de technologies à prérequis**
//! qui remplace les 4 branches linéaires (Essor/Terroir/Fer/Lien). Une nation détient
//! un **ensemble** de techs débloquées — un bitmask `u64` (bit `i` = tech d'id `i`). Une
//! tech est *recherchable* si **tous** ses prérequis sont acquis et que le savoir suffit.
//!
//! Les EFFETS sont une **fonction pure** des techs acquises (jamais stockés) :
//! [`effects`] plie la table `TREE` en un [`Effects`] cumulé, lu par S1/S2/S5/S8.
//! C'est l'unique source de vérité (UI + sim + IA la lisent). Déterminisme : aucun
//! hasard ; le choix de l'IA trie par (priorité, id croissant) → rejeu identique.

/// Effet élémentaire d'une techno (cumulé par [`effects`]). Les bonus s'ADDITIONNENT
/// (deux techs « +capacité » s'empilent) ; les déblocages sont des OU logiques.
#[derive(Debug, Clone, Copy)]
pub enum Effect {
    /// + bonus de **capacité de charge** des villes (0.4 = +40 % d'habitants).
    Capacity(f32),
    /// + **portée d'expansion** (budget de coût terrain de l'essaimage).
    Range(u32),
    /// + **influence** (multiplicateur du flux mensuel).
    Influence(f32),
    /// + production d'**industrie** (multiplicateur).
    Industry(f32),
    /// + production de **commerce** (argent + habitation).
    Commerce(f32),
    /// + production des **fermes** (nourriture).
    Farm(f32),
    /// + production de **science** (éducation).
    Science(f32),
    /// + production de **manpower** (casernes + ports).
    Manpower(f32),
    /// + **efficacité du travail** : postes requis ×(1 − Σ) → plus de bâtiments dotés.
    JobEff(f32),
    /// + **pollution** de l'industrie (0.5 = +50 % de dévastation par usine).
    Pollution(f32),
    /// Débloque la **traversée de l'eau** (unités navales + expansion outre-mer).
    Naval,
    /// Débloque le type d'unité **Archers**.
    Archer,
    /// Débloque le type d'unité **Cavalerie**.
    Cavalry,
    /// + **PV** des unités recrutées par la nation.
    UnitHp(i32),
    /// + **dégâts** des unités de la nation.
    UnitDmg(i32),
}

/// Une technologie de l'arbre.
pub struct Tech {
    /// Identifiant stable (= n° de bit dans le masque). Jamais réutilisé.
    pub id: u16,
    /// Nom affiché (français).
    pub name: &'static str,
    /// Palier d'âge (1..=4) — pour grouper l'affichage et la progression.
    pub age: u8,
    /// Savoir requis pour la débloquer.
    pub cost: f32,
    /// Prérequis (ids) — **tous** doivent être acquis.
    pub prereqs: &'static [u16],
    /// Effet en clair (UI).
    pub blurb: &'static str,
    /// Effets de jeu (cumulés à l'acquisition).
    pub effects: &'static [Effect],
}

use Effect::*;

/// L'arbre (single-source). Ids contigus 0.. ; prérequis pointant vers des ids plus
/// petits (le test `tree_is_a_valid_dag` le garantit). 4 âges, ~5 nœuds/âge.
pub const TREE: &[Tech] = &[
    // ───────────────────────── Âge I — Antiquité (racines) ─────────────────────────
    Tech { id: 0, name: "Agriculture", age: 1, cost: 30.0, prereqs: &[],
        blurb: "Fermes +50 % de nourriture.", effects: &[Farm(0.5)] },
    Tech { id: 1, name: "Maçonnerie", age: 1, cost: 30.0, prereqs: &[],
        blurb: "Villes +40 % d'habitants.", effects: &[Capacity(0.4)] },
    Tech { id: 2, name: "Roue", age: 1, cost: 30.0, prereqs: &[],
        blurb: "Expansion +40 de portée.", effects: &[Range(40)] },
    Tech { id: 3, name: "Bronze", age: 1, cost: 40.0, prereqs: &[],
        blurb: "Débloque les Archers ; unités +10 PV.", effects: &[Archer, UnitHp(10)] },
    Tech { id: 4, name: "Écriture", age: 1, cost: 30.0, prereqs: &[],
        blurb: "Éducation +50 % de savoir.", effects: &[Science(0.5)] },
    // ───────────────────────── Âge II — Antiquité classique ─────────────────────────
    Tech { id: 5, name: "Irrigation", age: 2, cost: 70.0, prereqs: &[0],
        blurb: "Fermes +60 % (cumulé).", effects: &[Farm(0.6)] },
    Tech { id: 6, name: "Aqueducs", age: 2, cost: 70.0, prereqs: &[1],
        blurb: "Villes +50 % (cumulé).", effects: &[Capacity(0.5)] },
    Tech { id: 7, name: "Monnaie", age: 2, cost: 70.0, prereqs: &[4],
        blurb: "Commerce +50 %, influence +25 %.", effects: &[Commerce(0.5), Influence(0.25)] },
    Tech { id: 8, name: "Forge", age: 2, cost: 80.0, prereqs: &[3],
        blurb: "Débloque la Cavalerie ; unités +10 PV/+10 dégâts.",
        effects: &[Cavalry, UnitHp(10), UnitDmg(10)] },
    Tech { id: 9, name: "Voile", age: 2, cost: 70.0, prereqs: &[2],
        blurb: "Franchit l'eau : ports, galères, expansion outre-mer.", effects: &[Naval] },
    Tech { id: 10, name: "Bureaucratie", age: 2, cost: 80.0, prereqs: &[1, 4],
        blurb: "Postes requis -20 % ; influence +25 %.", effects: &[JobEff(0.2), Influence(0.25)] },
    // ───────────────────────── Âge III — Époque médiévale ─────────────────────────
    Tech { id: 11, name: "Assolement", age: 3, cost: 140.0, prereqs: &[5],
        blurb: "Fermes +80 % (cumulé).", effects: &[Farm(0.8)] },
    Tech { id: 12, name: "Guildes", age: 3, cost: 150.0, prereqs: &[7, 10],
        blurb: "Industrie & commerce +50 % ; postes -20 %.",
        effects: &[Industry(0.5), Commerce(0.5), JobEff(0.2)] },
    Tech { id: 13, name: "Ingénierie", age: 3, cost: 140.0, prereqs: &[6],
        blurb: "Villes +60 % ; expansion +60.", effects: &[Capacity(0.6), Range(60)] },
    Tech { id: 14, name: "Cartographie", age: 3, cost: 130.0, prereqs: &[9],
        blurb: "Expansion +80 ; influence +25 %.", effects: &[Range(80), Influence(0.25)] },
    Tech { id: 15, name: "Chevalerie", age: 3, cost: 150.0, prereqs: &[8],
        blurb: "Unités +30 PV/+20 dégâts ; manpower +30 %.",
        effects: &[UnitHp(30), UnitDmg(20), Manpower(0.3)] },
    Tech { id: 16, name: "Université", age: 3, cost: 140.0, prereqs: &[7],
        blurb: "Éducation +70 % de savoir.", effects: &[Science(0.7)] },
    // ───────────────────────── Âge IV — Modernité ─────────────────────────
    Tech { id: 17, name: "Hygiène", age: 4, cost: 250.0, prereqs: &[13],
        blurb: "Villes +100 % (mégapoles).", effects: &[Capacity(1.0)] },
    Tech { id: 18, name: "Industrialisation", age: 4, cost: 280.0, prereqs: &[12],
        blurb: "Industrie +100 %, mais pollution +50 %.", effects: &[Industry(1.0), Pollution(0.5)] },
    Tech { id: 19, name: "Mécanisation", age: 4, cost: 250.0, prereqs: &[11],
        blurb: "Fermes +100 % ; manpower +20 %.", effects: &[Farm(1.0), Manpower(0.2)] },
    Tech { id: 20, name: "Banque", age: 4, cost: 270.0, prereqs: &[12],
        blurb: "Commerce +70 %, influence +50 %.", effects: &[Commerce(0.7), Influence(0.5)] },
    Tech { id: 21, name: "Conscription", age: 4, cost: 280.0, prereqs: &[15],
        blurb: "Manpower +70 % ; unités +20 PV.", effects: &[Manpower(0.7), UnitHp(20)] },
];

/// Effets cumulés d'un ensemble de techs (dérivé PUR, jamais stocké). Lu par les
/// formules de S1 (capacité), S2 (portée/naval), S5 (unités), S8 (production).
#[derive(Debug, Clone, Copy)]
pub struct Effects {
    pub capacity_mult: f32,
    pub range_bonus: u32,
    pub influence_mult: f32,
    pub industry_mult: f32,
    pub commerce_mult: f32,
    pub farm_mult: f32,
    pub science_mult: f32,
    pub manpower_mult: f32,
    pub job_eff: f32,
    pub pollution_mult: f32,
    pub naval: bool,
    pub archer: bool,
    pub cavalry: bool,
    pub unit_hp: i32,
    pub unit_dmg: i32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            capacity_mult: 1.0,
            range_bonus: 0,
            influence_mult: 1.0,
            industry_mult: 1.0,
            commerce_mult: 1.0,
            farm_mult: 1.0,
            science_mult: 1.0,
            manpower_mult: 1.0,
            job_eff: 0.0,
            pollution_mult: 1.0,
            naval: false,
            archer: false,
            cavalry: false,
            unit_hp: 0,
            unit_dmg: 0,
        }
    }
}

/// Plie la table en effets cumulés pour un masque de techs donné. Ordre d'itération
/// fixe (la table) + sommes/OU commutatifs → résultat déterministe.
pub fn effects(mask: u64) -> Effects {
    let mut e = Effects::default();
    for t in TREE {
        if mask & (1u64 << t.id) == 0 {
            continue;
        }
        for eff in t.effects {
            match *eff {
                Capacity(x) => e.capacity_mult += x,
                Range(x) => e.range_bonus += x,
                Influence(x) => e.influence_mult += x,
                Industry(x) => e.industry_mult += x,
                Commerce(x) => e.commerce_mult += x,
                Farm(x) => e.farm_mult += x,
                Science(x) => e.science_mult += x,
                Manpower(x) => e.manpower_mult += x,
                JobEff(x) => e.job_eff += x,
                Pollution(x) => e.pollution_mult += x,
                Naval => e.naval = true,
                Archer => e.archer = true,
                Cavalry => e.cavalry = true,
                UnitHp(x) => e.unit_hp += x,
                UnitDmg(x) => e.unit_dmg += x,
            }
        }
    }
    // L'efficacité du travail ne peut pas annuler tous les postes (garde-fou).
    e.job_eff = e.job_eff.min(0.75);
    e
}

/// La tech `id` est-elle acquise dans `mask` ?
pub fn is_researched(mask: u64, id: u16) -> bool {
    mask & (1u64 << id) != 0
}

/// La techno d'id `id` (si elle existe).
pub fn get(id: u16) -> Option<&'static Tech> {
    TREE.iter().find(|t| t.id == id)
}

/// La tech `id` est-elle **recherchable** depuis `mask` (existe, non acquise, tous
/// les prérequis acquis) ? (Ne vérifie PAS le savoir — c'est le sim qui le fait.)
pub fn can_research(mask: u64, id: u16) -> bool {
    if is_researched(mask, id) {
        return false;
    }
    match get(id) {
        Some(t) => t.prereqs.iter().all(|&p| is_researched(mask, p)),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_is_a_valid_dag() {
        // Ids contigus, uniques, et prérequis pointant vers des ids ANTÉRIEURS
        // (acycliques par construction) + tenant dans le masque u64.
        for (i, t) in TREE.iter().enumerate() {
            assert_eq!(t.id as usize, i, "ids contigus 0..");
            assert!(t.id < 64, "tient dans le masque u64");
            for &p in t.prereqs {
                assert!(p < t.id, "prérequis {p} antérieur à {} (DAG)", t.id);
            }
        }
    }

    #[test]
    fn effects_accumulate() {
        // Maçonnerie (+0.4) puis Aqueducs (+0.5) → capacité ×1.9.
        let mask = (1 << 1) | (1 << 6);
        let e = effects(mask);
        assert!((e.capacity_mult - 1.9).abs() < 1e-6);
        // Voile débloque le naval ; Forge la cavalerie.
        assert!(effects(1 << 9).naval);
        assert!(effects(1 << 8).cavalry);
        assert!(!effects(0).naval);
    }

    #[test]
    fn prereqs_gate_research() {
        // Aqueducs (prereq Maçonnerie) verrouillé sans Maçonnerie.
        assert!(!can_research(0, 6));
        assert!(can_research(1 << 1, 6));
        // Déjà acquis → non recherchable.
        assert!(!can_research(1 << 1, 1));
        // Racine sans prérequis → toujours recherchable au départ.
        assert!(can_research(0, 0));
    }
}
