# ENYO — Économie interne (S8) : plan

## RÉVISION (29/06) — influence ∝ taille + IA stratège

- **Influence = flux ∝ territoire + population** (et non plus `+1/mois` forfaitaire) :
  `INFLUENCE_BASE(3) + cases × INFLUENCE_PER_TILE(2) + pop_totale / INFLUENCE_POP_DIVISOR(1000)`
  (flux volontairement généreux : le joueur ne doit pas être étranglé en influence ;
  départ `STARTING_INFLUENCE = 60`).
  Plus une nation est grande et peuplée, plus elle rayonne — et plus elle peut
  s'étendre (l'expansion coûte de l'influence). Boucle vertueuse voulue. ✅ fait.
- **IA — développement économique plus intelligent & immersif** : priorités
  adaptatives + meilleure case (cf. la puce « IA — développement économique
  stratégique » plus bas), **éducation** (science→tech) et **infrastructure** (qu'elle
  ne bâtissait jamais), **Fer** recherché une fois militarisée (Archers→Cavalerie). ✅ fait.
- **IA AGRESSIVE (conquête émergente)** : l'IA s'arme **dès qu'un rival existe**
  (caserne tôt), recrute à **toutes** ses casernes (plafond ∝ 2× territoire, borné
  par argent+manpower), **s'étend vers le rival le plus proche** (les territoires se
  rejoignent), et **déclare la guerre dès qu'elle a une caserne**. Trois correctifs de
  fond ont débloqué la conquête : (a) **implantation sur le plus grand continent
  connexe** (les armées peuvent se rejoindre par terre) ; (b) **pathfinding Dijkstra**
  des unités (contourne les obstacles ; vise la case ennemie réellement *atteignable*,
  pas à vol d'oiseau) ; (c) **règle « au moins une case »** (sim) : une unité à pleins
  points franchit toujours une case adjacente, même si la météo/dévastation la rend
  plus chère que son budget — sinon les unités gelaient et n'atteignaient jamais
  l'ennemi. Résultat : guerres qui **aboutissent** (occupation → capitulation →
  annexion), empires qui montent et tombent. ✅ fait.

## RÉVISION (clarification du 28/06) — territoire ≠ villes

- **Territoire** = les cases possédées : la **zone** où l'on bâtit et où s'accumule
  l'**influence**. L'**expansion** étend le territoire et coûte de l'influence
  (l'ancien « essaimer » est renommé **« Étendre »** dans l'UI). ✅ fait.
- **Ville = un type de BÂTIMENT** (pas le territoire). Coûte **habitation + argent**
  (`build_cost(City) = (100 argent, 0 mat, 50 habitation)`). À sa fondation, des
  **colons** s'installent (`CITY_SEED_POP = 100`) pour amorcer la croissance. Une
  ville est la **seule** case où la population croît (logistique vers la capacité du
  terrain) ; c'est aussi la « case d'habitation » à laquelle les autres bâtiments se
  connectent. ✅ **fait (tranche B)**.
- **Ferme** = bâtiment : **produit de la nourriture** (rendement ∝ terrain :
  humidité/pluie, température, sol), coûte matériaux + argent, exige une population
  connectée. ✅ **fait (tranche A)**.
- **Nourriture** = ressource. **Toute la population mange chaque mois**, AU-DELÀ d'un
  **seuil de subsistance par case** (`SUBSISTENCE_PER_TILE = 1500` : les petites
  implantations se nourrissent seules ; seules les **villes denses** réclament des
  fermes). 1 nourriture nourrit `CITIZENS_PER_FOOD = 100` habitants/mois ; tout
  surplus est reporté (stock tampon). ✅ fait.
- **Famine** : sans réserve suffisante, la population **non nourrie décline**
  (`FAMINE_DECLINE = 0.25`/mois de la part non nourrie) et la case se **dévaste** un
  peu (`FAMINE_DEVASTATION`, signal organique lu par le Directeur). Une ville dense
  privée de fermes **reflue vers la subsistance**. Remplace l'urbanisation auto d'E5.
  ✅ **fait (tranche B)**.
- **Croissance = VILLES UNIQUEMENT + FAMINE** (choix joueur). ✅ **fait (tranche B)**.
- **Démolir / remplacer** : on peut **démolir** le bâtiment d'une case (puis bâtir
  autre chose). Remboursement = **moitié du coût × état de la case (1 − dévastation)**
  : une case ravagée rend moins. (`Command::Demolish`.) ✅ fait.
- **Pollution lente** : une industrie n'abîme la case que **très lentement** (sur
  plusieurs décennies) — `INDUSTRY_POLLUTION` faible + résorption lente. ✅ fait.
- **Manpower (« force ») = ressource de NATION** (`Nation.manpower`, remplace l'ancien
  `Tile.force`) : produite par les **casernes** (et **ports**), dépensée pour
  **recruter** des unités et les **régénérer**. Une unité sur son **territoire
  national** récupère `UNIT_REGEN_HP`/mois en consommant du manpower (pas en terre
  étrangère/neutre). ✅ fait.
- **IA & genèse** : chaque nation démarre sur une case **accueillante** (capacité
  ≥ `HOSPITABLE_CAP = 1500`, pour croître au-delà de 1000 et s'étendre — pas de
  soft-lock), **tirée aléatoirement mais de façon seedée** (même graine ⇒ même
  placement ⇒ rejeu identique). `spawn_nations` y **fonde une ville** (départ avec
  `STARTING_HOUSING = 60`) peuplée d'emblée à **1500 hab** (= seuil de subsistance,
  donc pas de famine instantanée, et ≥ 1000 = seuil d'expansion) : la nation peut
  **s'étendre dès le tour 0** au lieu d'attendre ~25 mois que la ville monte de 300 à
  1000. Vaut pour le joueur ET les IA.
- **IA — développement économique stratégique** (un bâtiment/tour, déterministe) :
  au lieu d'une chaîne figée, l'IA choisit le **bâtiment le plus utile** et la
  **meilleure case** selon sa situation. Souhaits ordonnés : amorcer chaque maillon
  (ville → industrie → ferme → **caserne si menacée** → commerce → **éducation**),
  puis **équilibrer au prorata des villes** (fermes pour nourrir, commerce, industrie,
  éducation pour la science→tech), **connecter** un territoire étalé (**infrastructure**),
  sinon **grandir** (ville). On bâtit le **premier souhait abordable** (pas de blocage).
  Placement : villes/fermes sur la **terre la plus fertile** (capacité max), le reste
  sur la moins fertile (on préserve la bonne terre). **Militarisation agressive** :
  caserne **dès qu'un rival existe**, recherche **Fer** (Archers→Cavalerie),
  recrutement à **toutes** les casernes (plafond ∝ 2× territoire), **expansion vers le
  rival** et **guerre déclarée tôt**. Recherche : Terroir/Essor/Lien en continu,
  **Fer** si elle a une caserne. ✅ **fait**.

> Le reste du document décrit la conception initiale ; en cas de divergence, la
> section RÉVISION ci-dessus prime.

---



> Document de design. Fait autorité avec `PLAN.md` / `docs/GAMEPLAY.md` / `CLAUDE.md`.
> Statut : **proposé**. Implémentation par tranches verticales testées (§8).
> Toute décision qui change ici se répercute dans `PLAN.md` au même commit.

## 1. Vision en une phrase

Le joueur **bâtit une économie de cases spécialisées** (industrie, commerce,
infrastructure, éducation, militaire) reliées en **réseaux**, qui transforment des
**ressources** (argent, matériaux, influence, science, habitation) ; la production
dépend des **stats de la case** et de la **population connectée**, et l'industrie
**pollue** (dévastation). La tech améliore et spécialise les cases.

## 2. Ressources (stocks par nation, **entiers `i64`** → déterminisme sans dérive)

| Ressource | Produite par | Consommée par | Base |
|---|---|---|---|
| **argent** | commerce, (impôt de base léger) | construire, entretien mensuel (militaire, éducation, infra) | départ ~500 |
| **matériaux** | industrie | construire, commerce | 0 |
| **influence** | flux/mois **∝ territoire + population** (plancher de base) | étendre le territoire (essaimer), agrandir les villes | 0 |
| **science** | éducation (+ base densité héritée) | recherche tech | = `knowledge` actuel (réutilisé) |
| **habitation** | commerce | loger/croître la population, fonder/essaimer | 0 |

> `science` réutilise le champ `Nation.knowledge` (tech le dépense déjà). On garde
> un petit flux de base par densité (legacy) ; l'éducation devient la vraie source.
> Les autres ressources sont de **nouveaux champs `i64`** sur `Nation`.

## 3. Cases (bâtiments) — `Tile.building: Option<Building>`

`Building ∈ { Industry, Commerce, Infrastructure, Education, Military }` (dans
`proto`). Une case **habitation** = case possédée **sans** bâtiment, qui porte la
`population` (modèle existant S1). Une case ne porte qu'**un** bâtiment.

| Bâtiment | Coût (construire) | Entretien /mois | Produit /mois | Exige | Effet de bord |
|---|---|---|---|---|---|
| **Industrie** | argent + matériaux | — | **matériaux** ∝ stats case × pop connectée × (1−dévastation) | connexion à une **habitation** | **+dévastation** /mois (pollution) |
| **Commerce** | argent + matériaux | — | **argent + habitation** + croissance pop, ∝ matériaux consommés × pop connectée | connexion habitation | — |
| **Infrastructure** | argent + matériaux | argent (léger) | — (connecte les cases) | — | étend les réseaux |
| **Éducation** | argent + matériaux | argent | **science** ∝ pop connectée | connexion habitation **et** commerce | — |
| **Militaire** | argent + matériaux | argent | **force** (soldats) sur la case | connexion habitation | — |

Production **mise à l'échelle** par la **population connectée** et par **(1 − dévastation)**.
Calibrage initial dans le code (consts `const`, single-source), à régler par golden.

## 4. Primitive de CONNEXION (le réseau) — nouvelle agrégation pure

Règle (du brief) : deux cases sont **connectées** si elles sont **adjacentes**,
**ou** toutes deux reliées au **même réseau d'infrastructure**. Une infra relie
transitivement toutes les cases collées à elle et à toute infra connectée.

**Implémentation** (déterministe, ordre d'index — comme la primitive provinces S4) :
union-find sur les cases possédées d'une nation :
1. unir deux cases possédées **adjacentes** (« à côté, ça marche ») ;
2. pour chaque **composante d'infrastructure** (flood-fill des infra adjacentes),
   unir **toutes** les cases possédées adjacentes à cette composante.
→ des **grappes** (clusters) de cases mutuellement connectées.

- **population connectée** d'un bâtiment = somme des `population` de sa grappe.
- un bâtiment « fonctionne » si sa grappe contient une **habitation** (pop > 0)
  (et pour l'éducation : aussi une **commerce**).
- recalculée chaque tour (fonction pure, jamais stockée — règle d'or).

## 5. Résolution mensuelle (dans `resolve_turn`, ordre canonique, entiers)

Par nation, après la dynamique S1 :
`influence += INFLUENCE_BASE + cases × INFLUENCE_PER_TILE + pop_totale / INFLUENCE_POP_DIVISOR`
(territoire + population pèsent → boucle vertueuse : grande nation peuplée = plus
d'influence = plus d'expansion ; somme pop en ordre d'index, quotient entier → rejeu
exact). Puis pour chaque
case bâtie (somme de contributions **arrondies par case** → `i64`, ordre d'index) :
- **Industrie** : `matériaux += round(BASE_IND × terrain(case) × workforce × (1−dev))` ;
  `devastation += POLLUTION` (borné). `terrain` dérive de `soil_fertility`,
  `vegetation`, `precip_now` (intempéries). `workforce = min(1, pop_connectée/SEUIL)`.
- **Commerce** : consomme des matériaux dispo → `argent +=`, `habitation +=`, pousse
  la croissance pop des habitations de la grappe.
- **Éducation** : paye l'entretien (argent) → `science +=` (∝ pop connectée).
- **Militaire** : paye l'entretien → `force +=` sur la case.
- **Infra** : paye l'entretien.

Entretien impayé (argent insuffisant) → le bâtiment **chôme** ce mois (pas de prod)
— jamais d'argent négatif.

## 6. Commandes & événements (event-sourcing)

- `Command::Build { x, y, nation, building }` → valide (possédée, vide, **abordable**),
  déduit le coût, pose le bâtiment. Sinon `CommandRejected`.
- (plus tard) `Command::Demolish { x, y, nation }`.
- Essaimer/Fonder : **coûtent influence (+ argent/matériaux/habitation)** — phase E5.
- Événement `Built { x, y, nation, building }`. Les ressources s'accumulent dans
  `resolve_turn` (comme `knowledge` : pas d'event par ressource ; le **checksum**
  couvre l'état). **`checksum` doit inclure `building` + les nouvelles ressources.**

## 7. Tech (science) — hooks

L'arbre 4 branches existant s'étend de techs économiques (améliorer/specialiser
industrie & commerce, +influence, +portée réseau, réduire pollution…). Détaillé en
E5/E6 ; coûts en **science**.

## 8. Roadmap par tranches (chacune shippable + golden)

- **E1 — Fondation ressources + Industrie.** `Nation { money, materials, influence }`
  (i64, argent de départ) ; `influence += 1/mois` ; `Building` enum + `Tile.building` ;
  `Command::Build`(Industrie) avec coût/déduction ; **production d'industrie** (stats
  case × **pop adjacente** × (1−dev)) + **pollution** ; `checksum` étendu. UI : outil
  **Bâtir/Industrie**, HUD ressources, inspecteur (bâtiment). *(Pop connectée = adjacence
  directe en E1 ; le réseau infra arrive en E2.)*
- **E2 — Connexion (primitive) + Infra + Commerce.** Union-find grappes (adjacence +
  réseau infra) ; production indexée sur la **pop connectée** ; bâtiments Infrastructure
  & Commerce (matériaux → argent + habitation + croissance).
- **E3 — Éducation & science.** Case Éducation (exige habitation + commerce) → science ;
  tech payée en science ; base densité conservée.
- **E4 — Militaire.** Case Militaire → soldats/mois + entretien argent.
- **E5 — Expansion économique.** Essaimer/Fonder coûtent influence + ressources ;
  croissance pop gâtée par habitation ; premières techs économiques.
- **E6 — UI complète & polish.** Menu de construction, panneau ressources détaillé,
  overlay de connectivité (réseaux), lecture de production par case, calibrage.

## 9. Déterminisme & minimalisme

- Ressources **entières** ; contributions **arrondies par case** puis sommées (ordre
  d'index) → indépendant de l'ordre, rejouable au bit près. Connexion = union-find en
  ordre d'index. `checksum` étendu (sinon divergence de rejeu non détectée).
- Le système reste **une** couche : un champ `building` par case + des stocks par
  nation + **une** primitive de connexion + des **fonctions pures** de production. Pas
  de jauge fourre-tout ; consts de calibrage **single-source**.

## 10. Questions ouvertes
1. Ressources **par nation** (retenu) ou par province (S4) ? — par nation d'abord.
2. `science` = `knowledge` réutilisé (retenu) ou champ séparé ?
3. La croissance pop doit-elle **dépendre de l'habitation** dès E2, ou rester S1 pure
   jusqu'à E5 ? — rester S1 jusqu'à E5 (éviter de casser l'équilibre tôt).
4. Calibrage des coûts/productions (à régler par golden au fil des phases).
