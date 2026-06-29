# ENYO — Plan de développement

> **Statut : v0.4 — document vivant.** Il évoluera à chaque décision de design.
>
> **Légende :** ✅ Décidé · 🔶 Proposé (à valider) · ❓ Ouvert (à décider)

---

## 1. Vision

Jeu de **stratégie minimaliste à l'échelle d'un monde entier**, croisant :

- la **profondeur de simulation émergente** de *Dwarf Fortress* / *Songs of Syx* ;
- la **grande stratégie** d'*Europa Universalis* / *Civilization* (nations, diplomatie, économie, guerre dans le temps) ;
- une **IA « Directeur » (DeepSeek V4)** qui met la partie en scène — **de façon invisible** — pour maximiser l'intérêt du joueur.

Le joueur est un **acteur** : il fait croître et essaimer sa civilisation et affronte les autres. Langage : **Rust**. Visuel et UI : **plus tard**. Priorité absolue : un **moteur headless, déterministe et entièrement auditable**, pour développer et tester sans interface graphique.

---

## 2. Périmètre & décisions actuelles

| Sujet | Décision | Statut |
|---|---|---|
| Échelle | Monde entier | ✅ |
| Forme du monde | **Plat, planisphère** — rectangle borné (pas de sphère) | ✅ |
| Résolution | **800 × 500 = 400 000 cases** (provisoire, extensible) | ✅ |
| Représentation | Grille de cases, stats par couches | ✅ |
| Rôle du joueur | **Acteur** — fait croître & essaimer sa civilisation | ✅ |
| Boucle cœur | Implantation → croissance → essaimage à **1000 pop fixe** : population **divisée 50/50**, cible **choisie par le joueur** (auto pour les IA) | ✅ |
| Portée d'essaimage | Dépend de la **technologie** (budget de coût terrain) | ✅ |
| Technologie | **Arbre à 4 branches thématiques** (niveau nation) ; les paliers = les âges | ✅ |
| Économie (MVP) | **Minimal** : développement + capacité + savoir (marchand → Phase 6) | ✅ |
| Design gameplay | **7 systèmes cœur** (voir `docs/GAMEPLAY.md`) | ✅ |
| Rythme | **Tour par tour** | ✅ |
| Granularité population | **Statistique par case** (agrégée) | ✅ |
| Modèle d'IA | **Directeur DeepSeek (invisible)** — pas contrôleur direct des nations | ✅ |
| Accès DeepSeek | **API cloud**, clé via `DEEPSEEK_API_KEY` (`.env` non versionné) | ✅ |
| Langage | Rust | ✅ |
| Audit | Journalisation **exhaustive** + replay | ✅ |
| Génération du monde | **Procédurale** (seedée) | ✅ |
| Durée d'un tour | **1 mois** | ✅ |
| Victoire | **Sandbox d'abord**, conditions plus tard | ✅ |
| IA pas chère par ennemi | **Pas au début** (règles + ton du Directeur) | ✅ |
| Multijoueur | **Non** (solo) | ✅ |
| Déterminisme | **Reproductible** (RNG + I/O LLM enregistrés) ; **fixed-point + ordre canonique** pour les ops spatiales ; bit-exact multi-plateforme non exigé | ✅ |
| Log | **JSONL** d'abord, SQLite ensuite | ✅ |
| Wrap est-ouest du monde | **Cylindre est-ouest** (bord droit relié au gauche) | ✅ |
| Granularité capacité/famine | **Par case** (la famine = pop > capacité) | ✅ |

---

## 3. Boucle de jeu (gameplay cœur)

Le joueur fait croître et essaimer sa civilisation sur la grille.

**Boucle de base :**
1. **Implantation** — le joueur choisit une **case de départ** et y **génère de la population**.
2. **Croissance** — la population de la case augmente chaque tour (stat `population_growth`, §4.6).
3. **Essaimage** — dès qu'une case atteint **1000 de population**, le joueur peut **s'étendre vers une autre case** : la population est **divisée entre les deux cases**.
4. **Portée** — la **distance** de la case cible d'essaimage dépend du niveau de **technologie**.
5. **Technologie** — un **arbre de technologie** se débloque au fil de la partie ; il augmente la portée d'essaimage (et d'autres effets à définir).

✅ Décidé : partage **50/50**, cible **choisie par le joueur** (auto pour les IA), seuil **1000 fixe**, monde **cylindrique** (wrap est-ouest), arbre de tech à **4 branches**. Le design complet (7 systèmes cœur, synergies, conflits, playbook du Directeur) est dans **`docs/GAMEPLAY.md`**.

---

## 4. Modèle du monde

### 4.1 La carte

Le monde est **plat**, en projection **planisphère** : un **rectangle borné** de **800 cases de largeur × 500 cases de hauteur = 400 000 cases** (valeur provisoire). Monde plat ⇒ aucune distorsion polaire à gérer.

Une case est l'unité physique de base : le **substrat** sur lequel se construisent provinces, nations et populations (§4.7). Une case est **terrestre** ou **aquatique** (`tile_kind`), ce qui détermine les stats applicables. ❓ Wrap est-ouest : à décider (§7).

### 4.2 Les couches de données d'une case

Les stats sont rangées par **fréquence de changement** : le moteur ne recalcule que ce qui bouge à un tour donné.

| Couche | Change… | Exemples |
|---|---|---|
| **Géologie** | quasi jamais (généré au départ) | altitude, relief, type de case, salinité |
| **Climat** | lentement, par saison | température moyenne, précipitations |
| **Biosphère** | sur des mois/années | couvert végétal, fertilité, faune |
| **Météo** | court terme (chaque tour) | température réelle, pluie/neige, vent |
| **Anthropique** (civilisation) | chaque tour, selon les actions | population, croissance, développement, dévastation |

### 4.3 Stats d'une **case de terre**

> *Origine* : « (liste) » = vient de ta description, renommé ; « (proposé) » = ajout cohérent à valider.

| Champ (code) | Nom | Échelle | Description | Origine |
|---|---|---|---|---|
| `tile_kind` | Type de case | enum | Terre / Océan / Lac / Côte / Rivière | (proposé) |
| `elevation` | Altitude | mètres (signé) | Hauteur vs niveau de la mer | (liste) |
| `ruggedness` | Relief | 0–1 | Rugosité du terrain : 0 = plaine, 1 = montagneux. **Indépendant de l'altitude** | (liste) « vallonné » |
| `mean_temperature` | Température moyenne | °C | Climat de base (latitude + altitude + saison) | (liste) « température » |
| `precipitation` | Précipitations | mm/an (ou 0–1) | Quantité d'eau reçue → désert ↔ forêt humide | (liste) « humidité » |
| `vegetation_cover` | Couvert végétal | 0–1 | Densité de végétation, **dynamique** | (liste) « végétation » |
| `soil_fertility` | Fertilité du sol | 0–1 | Potentiel agricole | (proposé) |
| `wildlife` | Faune terrestre | 0–1 | Abondance de gibier / ressources animales | (proposé) |
| `biome` | Biome | enum | Classification **dérivée** (désert, toundra, steppe, forêt…) | (proposé) |
| `weather` | Météo | struct (§4.5) | Conditions courantes | (liste) « temps » |

### 4.4 Stats d'une **case d'eau**

| Champ (code) | Nom | Échelle | Description | Origine |
|---|---|---|---|---|
| `tile_kind` | Type de case | enum | Océan / Lac / Côte | (proposé) |
| `elevation` | Altitude | mètres (négatif) | Profondeur du fond sous le niveau de la mer | (proposé) |
| `depth` | Profondeur | mètres | Vue pratique = \|elevation\| | (proposé) |
| `water_temperature` | Température de l'eau | °C | Distincte de la température de l'air | (liste) « température » |
| `salinity` | Salinité | 0–1 | Mer salée / lac d'eau douce ; influe pêche & irrigation | (proposé) |
| `current` | Courant marin | vecteur (dir + force) | Influe navigation, commerce, migration | (proposé) |
| `marine_life` | Faune marine | 0–1 | Abondance de poissons / ressources halieutiques | (liste) « faune » |
| `aquatic_vegetation` | Végétation aquatique | 0–1 | Plancton / algues — productivité primaire | (liste) « végétation » |
| `weather` | Météo | struct (§4.5) | Conditions courantes | (liste) « temps » |

### 4.5 La structure `weather` (Météo, partagée terre & eau)

| Champ | Nom | Description |
|---|---|---|
| `temperature` | Température réelle | Température du moment (≠ moyenne climatique) |
| `precipitation_now` | Précipitations actuelles | Pluie / neige en cours |
| `wind` | Vent | Vecteur direction + force |
| `cloud_cover` | Couverture nuageuse | 0–1 |

🔶 La météo est la couche la plus coûteuse (change chaque tour) : granularité et fréquence à calibrer ; on pourra la simuler par **régions météo** plutôt que case par case.

### 4.6 Stats anthropiques (population · croissance · développement · dévastation)

Évoluent **chaque tour** selon les actions des joueurs et la dynamique de croissance.

| Champ (code) | Nom | Échelle | Description | Origine |
|---|---|---|---|---|
| `population` | Population | habitants | Population présente sur la case | (liste) |
| `population_growth` | Croissance de population | %/tour (ou 0–1) | Taux de croissance de la pop sur la case (dérivé du terrain, du développement, du climat…) | (liste) |
| `development` | Développement | 0–1 (ou niveaux) | Niveau d'aménagement / d'exploitation de la case | (liste) |
| `devastation` | Dévastation | 0–1 | Dégâts subis. Sources : **intempéries** + **combats** | (liste) |

**Dynamique de croissance du développement** (conceptuel, 🔶 à affiner) — dépend de :

- la **population sur la case** ;
- la **somme des populations des cases voisines** ;
- les **stats de terrain** de la case (fertilité, relief, biome…) ;
- les **stats de météo/climat** de la case.

> `Δ développement(case) = f( population, Σ population_voisines, terrain, météo )`

🔶 La **dévastation** freinera la croissance et/ou réduira population & développement — mécanique précise à définir.

### 4.7 Du terrain aux nations (hiérarchie d'abstraction)

L'IA Directeur ne raisonne jamais sur 400 000 cases, mais sur un **état agrégé** :

```
Case (substrat : terrain, climat, biosphère, météo, population/développement)
  └─ Région / Province  (groupe contigu de cases — agrège les stats)         🔶
       └─ Nation  (joueur ou non-joueur — possède des provinces — ACTEUR)    🔶
            └─ IA Directeur (DeepSeek), AU-DESSUS de toutes les nations :
               donne le ton et les objectifs des ennemis pour servir
               l'intérêt du joueur (§5.5)
```

🔶 à valider.

---

## 5. Architecture technique

### 5.1 Principes directeurs

1. **Headless-first.** La simulation tourne sans rendu, pilotable en CLI. L'UI viendra se brancher dessus — jamais l'inverse.
2. **Déterminisme.** Même seed + mêmes commandes ⇒ même partie, rejouable au tour près. RNG seedé, pas de hasard caché.
3. **Event-sourcing.** Tout changement d'état passe par une **commande** → produit des **événements** → journalisés. L'état du monde = somme des événements.
4. **Audit total.** Chaque interaction est loguée dans un format structuré, requêtable et rejouable.
5. **Tranches verticales.** On construit système par système, de bout en bout (commande → logique → événement → log → test).

### 5.2 Découpage en crates (workspace Rust)

| Crate | Rôle | Dépend du rendu ? |
|---|---|---|
| `sim` | Cœur logique pur : monde, cases, systèmes, tour. Aucune I/O. | Non |
| `proto` | Types de commandes & d'événements partagés (le « langage » du jeu). | Non |
| `harness` | CLI / console pour piloter la sim, scénarios, replay, dumps. **Outil de test principal.** | Non |
| `ai` | IA Directeur (LLM DeepSeek) + IA ennemis (heuristique), cache, fallback. | Non |
| `persist` | Save/load, log structuré, snapshots. | Non |
| `ui` | Visualisation — **plus tard**, simple consommateur de `sim`. | Oui |

### 5.3 Boucle de simulation & déterminisme

- **Tour par tour**, déterministe. **1 tour = 1 mois.**
- **Ordre d'un tour :**
  1. **Tour du joueur** (ses actions).
  2. **L'IA Directeur observe tout** — elle « triche » (information complète, dont le tour du joueur) — et, **au début du tour des non-joueurs**, décide de la direction de la partie (§5.5).
  3. **Tours des nations non-joueuses**, qui agissent selon le ton donné par le Directeur.
  4. **Résolution du monde** : climat, météo, biosphère, croissance, dévastation…
- RNG **seedé** par partie ; tout tirage aléatoire passe par lui.
- Les couches de données (§4.2) se rafraîchissent à des cadences différentes pour la performance.
- Objectif **replay reproductible** (RNG + I/O LLM enregistrés), pas forcément bit-exact multi-plateforme.

### 5.4 Journalisation & audit (exigence centrale)

- **Log structuré** de chaque commande et événement → **JSONL** d'abord (simple, grep-able), SQLite ensuite.
- **Snapshots** : sérialisation complète de l'état du monde à n'importe quel tour.
- **Replay** : rejouer une partie depuis le log.
- **Scénarios de test** : scripts qui pilotent la sim et vérifient des invariants (« golden replays »).
- 🔶 Console / REPL intégrée pour piloter la sim à la main pendant les tests.

### 5.5 L'IA — modèle « Directeur » (game master invisible)

Le LLM (DeepSeek) **ne contrôle pas directement les nations ennemies**. C'est un **Directeur / metteur en scène** dont l'objectif est de rendre la partie **la plus intéressante possible pour le joueur** — **sans qu'il ne soupçonne jamais sa présence**.

**Deux niveaux d'IA :**

| Niveau | Qui | Rôle | Coût |
|---|---|---|---|
| **Directeur** | LLM DeepSeek | Donne le **ton** et la **direction** de la partie. « Triche » : voit tout. Décide **au début du tour des non-joueurs**. **1 appel par tour.** | Élevé, mais 1×/tour |
| **Ennemis** (🔶 plus tard) | IA « pas chère » par nation | Exécution tactique, dans le cadre fixé par le Directeur | Faible |

**Leviers du Directeur** ✅ : créer des **alliances** ; déclencher des **événements** (guerres, révoltes, raids, crises…) ; agir sur le **monde** (intempéries, dévastation, ressources, événements neutres).

**Objectif « intéressant »** ✅ — combiner : (1) **difficulté constante** (rubber-banding) ; (2) **drama narratif** (retournements, rivalités, trahisons, fils rouges).

**Contrainte impérative — invisibilité (« brouiller ses traces ») :**
- Le joueur **ne doit jamais remarquer** la main du Directeur ; toute manipulation a une **cause organique plausible** (*plausible deniability*).
- Le drama est orchestré **prioritairement entre les IA elles-mêmes** : le joueur perçoit un monde vivant, pas un marionnettiste braqué sur lui.
- Éviter les **motifs détectables** : varier, retarder, déguiser.
- **Conséquence d'archi** : on **sépare l'intention réelle du Directeur** (raisonnement caché, logué pour l'audit) de ce que **voit le joueur** (causes crédibles).

**Autres caractéristiques :**
- **Mémoire persistante** ✅ : fil narratif entre les tours (rivaux récurrents, intrigues).
- **Accès** : API cloud DeepSeek, clé via `DEEPSEEK_API_KEY` (`.env` **non versionné**). La clé n'apparaît jamais dans le dépôt.
- **Sorties contraintes** à un espace d'actions légal (sortie structurée / DSL).
- **Non-déterminisme géré par enregistrement** : I/O LLM loguées ⇒ replay reproductible.
- **Baseline déterministe** développée **avant** le LLM : référence, test, et **fallback**.

---

## 6. Roadmap par phases

> **Avancement (poussé sur GitHub) :** **Phases 0–5 ✅** (7 systèmes S1–S7 + IA baseline + **Directeur LLM DeepSeek**) + **Phase 7a ✅** (renderer **headless → PNG** : overview + zoom nation + **tileset pixel-art texturé** ; **time-lapse GIF** + planche-contact ; **`run.bat` 1-clic**). Reste : UI interactive (7b), profondeur. **43 tests** verts, clippy propre, **audit complet OK**.

- **Phase 0 — Fondations.** Workspace Rust, CI, logging (`tracing`), RNG seedé, harness minimal, contrat de déterminisme + premier test de replay.
- **Phase 1 — Le monde qui tourne.** Génération procédurale de la grille **800×500** + modèle de case (§4) + boucle de **tour**. Un monde géographique qui évolue.
- **Phase 2 — Boucle cœur.** Implantation, population & croissance, **essaimage**, développement, dévastation, **arbre de tech** (portée), de bout en bout (commandes/événements/logs/tests).
- **Phase 3 — Audit & replay complets.** Log requêtable, snapshots, replay total, scénarios de test.
- **Phase 4 — IA baseline déterministe.** Nations ennemies jouées par des règles ; référence et fallback.
- **Phase 5 — IA Directeur (DeepSeek V4).** Le metteur en scène (§5.5) : décisions au début du tour des non-joueurs, I/O loguées, budget/latence maîtrisés.
- **Phase 6 — Profondeur.** Diplomatie, militaire, économie avancée, culture/religion… selon le design retenu.
- **Phase 7 — UI / visuel.**

---

## 7. Questions ouvertes (à trancher)

La plupart des questions de design sont tranchées (voir §2 et `docs/GAMEPLAY.md`). Restent surtout des **calibrages**, à régler au moment du code :

- ❓ Valeurs numériques des **garde-fous anti-injustice** du Directeur (cap des nudges négatifs, cooldowns, seuil de péril) — à calibrer sur l'Indice de Drame avant de brancher le LLM.
- ❓ Contenu fin des **4 branches de tech** (quels modificateurs, à quels paliers).
- ❓ Stratégie précise de **ré-agrégation des provinces** (cadence, hystérésis) — à spécifier en Phase 3.

---

## 8. Conventions de nommage

- Identifiants de code en **`snake_case` anglais** ; noms d'affichage en français dans l'UI.
- Stats normalisées en **0–1** quand c'est une intensité/densité ; unités physiques (°C, m, mm) quand ça a un sens concret.
- Une stat = une couche claire (§4.2) ; pas de stat « fourre-tout ».
