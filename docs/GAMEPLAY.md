# ENYO — Design de gameplay (synthèse du fan-out)

> Issu d'une exploration multi-agents (8 dimensions × explore + critique adversariale → synthèse).
> **Résultat clé : 42 mécaniques proposées repliées sur 7 systèmes cœur.** C'est ça, le minimalisme.
>
> Statut : **proposition** — à valider/trancher (voir §8). Légende : ✅ garder · 🔁 retravailler · ⏳ Phase 6.

---

## 1. En une phrase

ENYO tient en **7 systèmes cœur** (hors substrat géographique), reliés par **une boucle** et **6 primitives partagées**. La capacité de charge était proposée 4×, le savoir 2×, le coût de terrain 2× — tout ça est fusionné.

---

## 2. Les 7 systèmes cœur

| # | Système | Phase | Ce qu'il absorbe |
|---|---|---|---|
| **S1** | **Physique de la case** — croissance logistique vers une *capacité de charge* dérivée du terrain ; famine = `pop > capacité` ; densité → congestion → débordement = **villes émergentes** | 2 | cité, famine (éco), saturation (pacing), capacité-tech |
| **S2** | **Essaimage & franchissement** — à 1000 pop, division 50/50 vers une case *choisie par le joueur*, atteignable dans un **budget de coût terrain** (tech-gated). S1 *gate* S2 → tension tall/wide native | 2 | carte (essaimage, coût, franchissement) |
| **S3** | **Savoir & arbre de tech** — savoir = flux *pur* des cases denses/développées (les villes) → finance un arbre compact de modificateurs. Les **paliers = les âges** | 2 | tech (×4), savoir-contagion, ères |
| **S4** | **Provinces émergentes** — flood-fill des cases connexes d'une nation. Agrégat unique lu par Directeur/diplo/militaire (« jamais 400k cases ») | 3 | hiérarchie Case→Province→Nation |
| **S5** | **Militaire** — la force est une *stat de case* ; mobilisation = curseur pop→force ; mouvement via la primitive d'essaimage ; combat déterministe sur case ; ravitaillement/attrition → terre brûlée ; bandes neutres → sécession | 4 | militaire (×6) |
| **S6** | **Diplomatie** — opinion = *fonction pure* (culture + frontière + commerce + griefs décroissants) ; traités = *prédicats* évalués chaque mois ; casus belli *mintés* par l'essaimage contesté | 4 | diplomatie (×4) |
| **S7** | **Directeur** — Indice de Drame (lecture seule) → Budget de Pression (∝ puissance *relative*) → Budget d'Équité (anti-acharnement). Ne biaise que les *entrées* de S1/S2/S6 → déniabilité par construction | 5 | directeur (×6), pacing |

**Substrat (Phase 1, pas un « système ») :** génération seedée + couches géologie/climat/biosphère/météo à cadences différentes. Tout le reste n'en est que *lecture pure*.

> **Révision économie (S8, juin) — S1/S2 mis à jour.** Le « peuplement » est dissocié
> du territoire : la **croissance de population se fait UNIQUEMENT sur les *villes***
> (une **ville est un bâtiment** de S8, pas le territoire) et elle est **bornée par la
> nourriture** — chaque case a un **seuil de subsistance** ; au-delà, la population doit
> être nourrie par des **fermes**, sinon **famine** (déclin). L'**« essaimage » de S2
> est renommé « expansion » (Étendre)** = revendiquer du territoire (coûte de
> l'influence) ; il déplace toujours des colons (main-d'œuvre). Modèle et calibrage
> **faisant autorité** : `docs/ECONOMY.md` § RÉVISION.

> **Révision militaire (S5) — UNITÉS.** **Tout le militaire passe par les unités**
> (Mobiliser/Marcher retirés) : la `force` (stat de case produite par les casernes)
> ne sert plus qu'à **recruter des unités** — des **agents discrets** (position, PV,
> dégâts, portée, points de mouvement).
> Recruter coûte **argent + force** ; les **types** (Infanterie, Archers, Cavalerie)
> sont débloqués par la branche **Fer**. Les **points de mouvement** dépendent du
> terrain ET des **intempéries** (pluie/dévastation/gel ralentissent — primitive
> `path::unit_move_cost`). Au combat, le **terrain du défenseur** donne un bonus de
> défense (végétation, relief, neige/pluie) et le **terrain de l'attaquant** peut
> donner un **malus** selon le type (archers en forêt, cavalerie en terrain
> accidenté). Tout en **entiers** (déterminisme). C'est la **première entorse**
> assumée au « tout en stats de case » : les unités sont des entités à part entière.

> **Révision conquête (S5/S6) — OCCUPATION & CAPITULATION.** Le **seul moyen de
> prendre du territoire ennemi** = l'**occuper** puis **gagner la guerre**. Quand une
> unité passe sur une case ennemie (en guerre), la case devient **hachurée**
> (occupée, **collante**) : elle reste à l'ennemi mais rapporte du **score de
> guerre** = la **valeur** des cases occupées (**vide 1 / bâtiment 5 / ville 10**).
> Dès que le score d'occupation dépasse **75 % de la valeur totale** de l'ennemi, il
> **capitule** : le vainqueur **annexe les cases qu'il occupe** et la **paix est
> imposée**. Le seuil étant relatif au total, **un grand empire est plus dur à faire
> plier** (une case parmi 200 pèse peu). La paix négociée (sans victoire) **n'annexe
> rien** (les occupations retombent). Le propriétaire **reprend** une case occupée en
> y ramenant une unité.

---

## 3. Les 6 primitives partagées (la vraie raison du minimalisme)

1. **Un seul état** : la stat de case + un journal append-only. Tout le reste = fonction pure rejouable.
2. **Une primitive de mouvement** (transfert d'essaimage) pour la **pop ET la force**.
3. **Une primitive d'atteignabilité** (budget de coût terrain) pour essaimage / migration / portée militaire.
4. **Une agrégation** (provinces) pour Directeur / diplo / militaire.
5. **Une monnaie de déclin** (`devastation`) couplant météo + combat + congestion + peste → baisse de capacité.
6. **Un carburant de progression** (savoir des cases denses).

---

## 4. Short-list scorée (23 mécaniques)

| Mécanique | Dim. | Score | Verdict | Phase |
|---|---|---|---|---|
| Capacité d'accueil & croissance logistique | cité | 4.67 | ✅ canonique S1 | 2 |
| Densité urbaine & malus d'agglomération | cité | 4.50 | ✅ | 2 |
| Savoir émergent (dérivé des cases) | tech | 4.50 | ✅ | 2 |
| Rivalité de frontière & casus belli émergents | diplo | 4.33 | ✅ | 4 |
| Ravitaillement & attrition organiques | mil. | 4.33 | ✅ | 4 |
| Bandes & soulèvements émergents (force neutre) | mil. | 4.33 | ✅ | 4 |
| Mobilisation (Levée vs Solde) | mil. | 4.33 | ✅ | 4 |
| Aléa climatique orienté | dir. | 4.33 | ✅ | 5 |
| Opinion dérivée (registre de griefs) | diplo | 4.17 | ✅ | 4 |
| Traités = règles permanentes à déclencheurs | diplo | 4.17 | ✅ | 4 |
| Marche & front émergent | mil. | 4.17 | 🔁 transfert explicite | 4 |
| Oubli & redécouverte (déclin tech) | tech | 4.17 | ⏳ | 6 |
| Dérive d'opinion (nudge Directeur) | dir. | 4.17 | ✅ | 5 |
| Pression migratoire orientée | dir. | 4.17 | ✅ | 5 |
| Essaimage par seuil | carte | 4.00 | 🔁 joueur choisit | 2 |
| Coût de terrain & franchissement | carte | 4.00 | ✅ | 2 |
| Provinces émergentes par agrégation | carte | 4.00 | ✅ | 3 |
| Arbre à 4 branches (paliers de savoir) | tech | 4.00 | ✅ | 2 |
| Résolution de combat sur la case | mil. | 4.00 | ✅ | 4 |
| Indice de Drame (signaux du Directeur) | dir. | 4.00 | ✅ | 5 |
| Budget d'Équité (anti-injustice + salut) | dir. | 3.67 | 🔁 | 5 |
| Théâtre diplomatique (intention vs cause) | diplo | 3.67 | ✅ (en dernier) | 5 |
| Germes de calamité (réduit à la PESTE) | dir. | 3.50 | 🔁 peste seule | 5 |

---

## 5. Synergies (les chaînes émergentes)

- **S1 *gate* S2** : une case sous-1000 ne peut jamais essaimer → la tech qui relève la capacité débloque d'un coup des centaines de cases = **booms coloniaux endogènes**. Le tempo est piloté par la tech, sans timer.
- **Villes = savoir** : les amas denses (S1) produisent le savoir (S3) → **raser les villes d'un rival le renvoie à l'âge sombre** (oubli S3). La géographie décide qui invente, la guerre décide qui désapprend.
- **Boucle cœur → guerre** : l'essaimage vers une case contestée (S2) *minte* le casus belli + un grief (S6) → chaque guerre est traçable à une case et un tour.
- **La guerre redessine la carte** : combat → devastation → capacité↓ (S1) → famine → bandes (S5) → sécession (S4/S6), sans script.
- **Terre brûlée** : l'attrition (S5) lit la devastation (S1) → augmenter sa *propre* devastation en reculant affame l'envahisseur. Profondeur née de stats déjà là.
- **Déniabilité gratuite** : tous les leviers du Directeur (S7) passent par des entrées qui *fluctuent déjà seules* (météo, score d'essaimage, griefs) → chaque action a une cause organique automatique, **aucun « système de déguisement » à coder**.

---

## 6. Conflits détectés & résolutions

- **Agentivité** : l'essaimage auto vers « la meilleure case » retire LA décision cœur du joueur. → **Le joueur choisit la cible** ; auto-scoring réservé aux IA.
- **Deux systèmes de mouvement de pop** (essaimage discret vs migration continue). → Migration restreinte à la **détresse** (devastation/croissance<0).
- **Empilement des vecteurs de déclin** (famine + congestion + décadence + épuisement + attrition = monde punitif). → Garder **≤ 2 sources** (capacité/famine + guerre).
- **Deux déterminations du propriétaire de case** (combat + champ d'influence) = bug déterminisme. → **Source unique** = propriétaire de case ; la province ne fait que lire.
- **`war_weariness` double-compte** (mobilisation + bandes + attrition le couvrent déjà). → Rétrogradé en **indicateur read-only** du Directeur.
- **Ré-agrégation des provinces à chaque flip de combat** = clignotement + coût CPU. → **Batch en fin d'étape monde + hystérésis + id déterministe**.
- **Couplage de la formule de capacité** (partagée S1/S2/S3/S5). → La **geler (API + golden tests) AVANT** de coder les dépendants.

---

## 7. Coupé pour le minimalisme (MVP)

- **Vocation de case** : dérivable à la volée (terrain + tech), pas de champ persisté.
- **Catalogue de bâtiments case-par-case** : c'est le micro de city-builder qu'ENYO refuse ; garder 2-3 *débloqueurs de terrain* (terrasses/irrigation) comme **effets de tech**.
- **Vivres/famine séparés** : famine = `pop > capacité`, pas de stock de nourriture.
- **Économie marchande** (4 flux / commerce pathfindé / gisements / boom-bust) → **Phase 6** (le commerce pathfindé est la mécanique la plus chère du dossier).
- **Ères/civ_index séparés** : repliés dans les paliers de tech (1 tier = 1 âge).
- **Frontières d'influence, colonisation outre-mer, vassalité, empreinte/épuisement, décadence-comme-stat** → Phase 6 ou repliés en fonctions pures.

---

## 8. Décisions (toutes tranchées ✅)

> Les 4 décisions de design ont été validées par l'humain ; les 4 décisions techniques sont adoptées sur reco du fan-out.

| # | Décision | ✅ Retenu |
|---|---|---|
| 1 | **Wrap est-ouest** (rectangle fermé vs cylindre) — *bloque* l'atteignabilité/provinces/fronts, à trancher EN PREMIER | Cylindre (mondes crédibles) ; fermé = plus simple MVP |
| 2 | **Essaimage** : agentivité, partage, seuil | Joueur choisit la cible ; 50/50 ; seuil 1000 **fixe** |
| 3 | **Granularité capacité/famine** : par case vs par province | Par **case** (débloque le dev avant S4) |
| 4 | **Forme de l'arbre de tech** : 4 branches vs fourche tall/wide explicite | **4 branches** (le tall/wide *émerge* de l'arbitrage) |
| 5 | **Périmètre éco du MVP** : dev+capacité+savoir vs +matériaux/commerce | **Minimal** ; marchand en Phase 6 |
| 6 | **Garde-fous anti-injustice** du Directeur (valeurs numériques) | À calibrer sur l'Indice de Drame avant le LLM |
| 7 | **Contrat de déterminisme spatial** : « reproductible » suffit vs imposer entiers/fixed-point + ordre canonique | **Imposer** fixed-point + ordre canonique (peu coûteux, blinde le replay) |
| 8 | **Ré-agrégation des provinces** sous flips de combat | Cadence + régions « sales » + hystérésis + id par graine |

---

## 9. Playbook du Directeur (comment il reste invisible)

**Une fois par tour** (1 appel LLM, début du tour des non-joueurs), le Directeur reçoit un **état compact** (provinces, opinions/rivalités/casus belli, niveaux de tech, + 5-6 signaux de l'Indice de Drame) — jamais 400k cases.

**Boucle de contrôle (3 étapes, une seule boucle) :**
1. **Indice de Drame** (rang relatif, momentum lissé, péril, ennui, volatilité) → donne le signe : le joueur domine / souffre / stagne.
2. **Budget de Pression** ∝ puissance *relative* du joueur → combien d'adversité dépenser. **C'est la courbe de difficulté** (pas une punition du leader en absolu).
3. **Budget d'Équité** → borne tout : cap glissant des coups négatifs, interdiction d'empiler, et **salut déguisé obligatoire** si effondrement *non mérité* (péril haut + momentum offensif faible).

**Principe d'invisibilité :** le Directeur n'a **aucune commande à effet spécial**. Il ne fait que **biaiser les entrées** de systèmes qui fluctuent déjà seuls → chaque effet a une cause organique automatique.

**Les leviers (tous routés dans S1/S2/S6) :**
- **Aléa climatique** (entrée S1) : sécheresse bornée ≤2σ sur le grenier d'un leader → famine émergente. Max 1 biais/province.
- **Biais d'essaimage** (entrée S2) : incline la cible d'une IA vers la frontière du joueur → friction → casus belli organique. Levier de fond lent.
- **Nudge d'opinion** (entrée S6) : terme caché minuscule sur 1-2 paires déjà tendues → coalition contre l'hégémon. L'IA nation décide elle-même et **refuse** si le rapport de force est absurde.
- **Peste** (entrée S1/S5) : seulement où les préconditions existent (dense + dev bas + devastation) → effondrement urbain → réfugiés → bandes. La densité rend la cause « la faute de la ville ».
- **Oubli** (S3, Phase 6) : destruction soutenue des villes d'un hégémon → âge sombre. Chute épique imputée aux villes perdues.

**Construire S7 dans l'ordre :** Indice de Drame → Budget d'Équité → leviers déterministes → **Théâtre LLM en dernier** (la baseline 100% déterministe doit être shippable seule ; le LLM ne fait que *choisir* parmi des actions déjà légales/bornées, avec `hidden_intent` logué vs `public_cause` organique).
