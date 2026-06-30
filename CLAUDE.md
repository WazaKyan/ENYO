# ENYO — Instructions projet (le « soul »)

Ce fichier oriente tout le travail de code sur ENYO. Les documents de design font autorité :
- **`PLAN.md`** — plan, décisions, modèle du monde, roadmap.
- **`docs/GAMEPLAY.md`** — les 7 systèmes cœur, synergies, playbook du Directeur.

En cas de doute, ces docs priment. Si une décision de design change, **mets-les à jour dans le même commit** que le code concerné.

---

## Le projet en une phrase

Jeu de stratégie **minimaliste**, monde entier, en **Rust**, où le joueur fait croître et essaimer sa civilisation sur une grille, sous l'œil d'une **IA « Directeur » (DeepSeek) invisible** qui met la partie en scène. Priorité : un **moteur headless, déterministe et entièrement auditable** — tout le développement et les tests se font **sans interface graphique**.

---

## Principes non négociables

1. **Headless-first.** Le cœur (`sim`) ne dépend JAMAIS du rendu. L'UI est un simple consommateur. Si une fonctionnalité a besoin de l'UI pour être testée, c'est un défaut de conception.
2. **Déterminisme.** Même seed + mêmes commandes ⇒ même partie, rejouable au tour près. RNG seedé unique ; **aucun hasard caché** ; **fixed-point / entiers + ordre de visite canonique** pour toute opération spatiale (Dijkstra, flood-fill, marche, diffusion). Pas de `f32` dans ces ops. Bit-exact multi-plateforme non exigé, mais le replay intra-machine doit être parfait.
3. **Event-sourcing.** Tout changement d'état = une **commande** → des **événements** → journalisés. L'état du monde est la somme des événements. Pas de mutation « sauvage ».
4. **Audit total.** Chaque interaction est loguée (JSONL d'abord). Snapshots sérialisables à tout tour. Replay complet depuis le log. C'est ce qui me permet de simuler et tester sans UI.
5. **Tranches verticales.** On construit **un système de bout en bout** (commande → logique → événement → log → test) avant de passer au suivant. Jamais des couches horizontales à moitié.
6. **Minimalisme militant.** Le design tient en **7 systèmes cœur** + **6 primitives partagées**. Avant d'ajouter quoi que ce soit, demander : « est-ce repliable sur une primitive existante ? ». Une stat = une couche claire ; **pas de stat/jauge fourre-tout** (ex. `war_weariness` est interdit comme système : c'est un indicateur dérivé).

---

## Les 7 systèmes cœur (détail dans `docs/GAMEPLAY.md`)

- **S1 Physique de la case** — croissance logistique vers une *capacité de charge* dérivée du terrain ; famine = `pop > capacité` ; densité → congestion → villes **émergentes**.
- **S2 Expansion (« Étendre ») & franchissement** — **REFONTE (EU5, 30/06)** : revendiquer une case terre **atteignable** (budget de coût terrain tech-gated) contre de l'**influence** ; **ne consomme PLUS de population** (fini le seuil 1000 et la division 50/50). La case revendiquée est **vide** ; on y bâtit une **ville** pour la peupler (la population ne vit que sur les villes — cf. refonte population en cours). Cible **choisie par le joueur** (auto pour les IA).
- **S3 Savoir & tech** — savoir = flux *pur* des cases denses → arbre à **4 branches** (paliers = âges).
- **S4 Provinces émergentes** — flood-fill des cases connexes d'une nation ; agrégat unique lu par Directeur/diplo/militaire.
- **S5 Militaire** — la force est une *stat de case* ; mouvement via la primitive d'essaimage ; combat sur case → `devastation`.
- **S6 Diplomatie** — opinion = *fonction pure* ; traités = *prédicats* mensuels ; casus belli *mintés* par l'essaimage contesté.
- **S7 Directeur** — voir ci-dessous.

**Les 6 primitives partagées** (à respecter pour rester minimaliste) : (a) un seul état = stat de case + journal append-only, le reste = fonctions pures ; (b) **une** primitive de mouvement (pop ET force) ; (c) **une** primitive d'atteignabilité (coût terrain) ; (d) **une** agrégation (provinces) ; (e) **une** monnaie de déclin (`devastation`) ; (f) **un** carburant de progression (savoir).

> Règle d'or : les **stats dérivées ne se stockent jamais** (capacité de charge, opinion, savoir…), elles se recalculent par fonction pure. La **formule de capacité de charge** est partagée par S1/S2/S3/S5 : la **geler (API + golden tests) AVANT** de coder les dépendants.

---

## L'IA Directeur (principe d'invisibilité)

- Le Directeur **ne contrôle pas** les nations et **n'a aucune commande à effet spécial**. Il **biaise seulement les entrées** de S1/S2/S6 (météo, score d'essaimage, opinion) qui fluctuent déjà seules ⇒ **chaque effet a une cause organique automatique** (déniabilité par construction).
- Boucle : **Indice de Drame** (lecture seule) → **Budget de Pression** (∝ puissance *relative* du joueur) → **Budget d'Équité** (anti-acharnement + salut déguisé si effondrement non mérité).
- **Ordre de construction :** Indice de Drame → Budget d'Équité → leviers déterministes → **couche LLM en dernier**. La baseline 100 % déterministe doit être *shippable seule* ; le LLM ne fait que **choisir** parmi des actions déjà légales/bornées, en attachant `hidden_intent` (logué, audit) vs `public_cause` (organique, vu du joueur).
- 1 appel LLM/tour, au début du tour des non-joueurs, sur un **état agrégé** (jamais 400k cases). I/O LLM **enregistrées** → replay reproductible.

---

## Architecture & crates (workspace Rust)

| Crate | Rôle | Rendu ? |
|---|---|---|
| `sim` | Cœur logique pur : monde, cases, systèmes, tour. Aucune I/O. | Non |
| `proto` | Types de commandes & d'événements partagés. | Non |
| `harness` | CLI/console : piloter la sim, scénarios, replay, dumps. Outil de test principal. | Non |
| `ai` | Directeur (LLM DeepSeek) + IA ennemis (heuristique), cache, fallback. | Non |
| `persist` | Save/load, log structuré, snapshots. | Non |
| `ui` | Visualisation — **plus tard**, consommateur de `sim`. | Oui |

---

## Conventions de code (Rust)

- Identifiants en **`snake_case` anglais** ; chaînes d'affichage en **français**.
- Stats normalisées **0–1** pour les intensités/densités ; unités physiques (°C, m, mm) quand concret.
- **Fonctions pures** pour tout dérivé ; pas d'effet de bord hors du pipeline commande→événement.
- Une **formule centralisée et versionnée** par concept partagé (capacité, coût terrain…) — ne pas dupliquer les coefficients.
- Préférer des **tables de modificateurs `const`** (tech, vocations) à du code spécial par cas.
- Erreurs explicites (`Result`), pas de `unwrap()` en chemin de simulation.

## Déterminisme — le contrat

- RNG seedé unique passé explicitement ; jamais de RNG global ad hoc.
- Ops spatiales : entiers/fixed-point + ordre canonique (index croissant). Interdire `f32` ici.
- Tout non-déterminisme externe (LLM) est **enregistré** dans le log et **rejoué** au replay.
- Trancher le **wrap est-ouest = cylindre** est acté : l'adjacence boucle sur X.

## Tests & audit

- **Golden replays** : un scénario + un seed ⇒ un log/état de référence ; tout changement de comportement doit être intentionnel et re-béni.
- Geler par des tests les **formules partagées** avant d'écrire leurs dépendants.
- Le `harness` doit pouvoir : charger un seed, exécuter N tours, dumper l'état/le log, rejouer un log.

---

## Workflow de développement

- Avancer par **petites tranches verticales testées**, dans l'ordre de la roadmap (`PLAN.md` §6). Phase courante : **0 (Fondations)** puis **1 (le monde qui tourne)**.
- **Tenir les docs à jour** : toute décision de design ⇒ mise à jour de `PLAN.md`/`docs/GAMEPLAY.md` dans le même commit.
- **Git** : commits petits et fréquents sur `main` (repo solo). Messages clairs. Pousser quand l'utilisateur le demande.
- Ne pas introduire de dépendance lourde sans raison ; rester proche de l'esprit minimaliste.

## Secrets

- La clé DeepSeek vit dans **`.env`** (ignoré par git) via `DEEPSEEK_API_KEY`. **Ne jamais** écrire de secret dans un fichier versionné, un log, ou un message. `.env.example` documente la variable.

---

## Rappels de design figés

- Monde **plat, planisphère, cylindre est-ouest**, **800 × 500 = 400 000 cases**.
- **Tour par tour, 1 tour = 1 mois.** Solo. Sandbox d'abord.
- `population`, `population_growth`, `development`, `devastation` sont des **stats de case** (agrégées, pas d'agents individuels).
- Ordre d'un tour : joueur → Directeur (observe tout, décide la direction) → nations non-joueuses → résolution du monde.
- Économie MVP **minimale** : développement + capacité + savoir (marchand en Phase 6).
