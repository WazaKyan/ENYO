# Audit cruel — Économie E1 (S8), commit `a5e26ea`

> Audit adversarial multi-agents (34 agents, 4 dimensions → vérification sceptique).
> **29 trouvailles, 21 confirmées — TOUTES de sévérité basse**, 4 non-problèmes.

## Verdict

**E1 est une fondation SAINE pour E2.** Aucun bug runtime, aucun crash, **aucun
exploit à avantage, aucune rupture de déterminisme**. Tous les non-négociables
tiennent : record→replay prouvé (`7a896bb3`), event-sourcing (tout via
`World::apply`), dérivés purs non stockés, ressources = **stocks légitimes** (pas
des dérivés), consts single-source, **pas de `f32` en op spatiale** (le `f32`
d'`industry_output` est un scalaire par case quantifié `i64`), pas d'`unwrap` en
chemin de sim. Toutes les trouvailles sont des **raffinements**.

## Corrigé (commit de suivi)

- **Fail-closed** : `build()` refuse désormais les bâtiments non encore implémentés
  (Éducation = E3, Militaire = E4) et les cases non-terre — plus de ressources
  brûlées en silence. *(Commerce/Infra produisent depuis E2.)*
- **Pollution conditionnelle** : une industrie à **production nulle ne pollue plus**
  (corrige le piège « 0 matériau + dégrade quand même la case »).
- **Influence aux nations vivantes** : seules les nations possédant ≥ 1 case
  gagnent de l'influence (plus de crédit aux nations conquises).
- **Affichage français** : `building_fr()` — l'inspecteur et les retours d'action
  montrent « Industrie/Commerce/… » au lieu des noms `Debug` anglais.
- **Formules gelées (golden)** : `industry_yield()` (extrait, pur, testé) et
  `build_cost()` figés par des tests de valeurs exactes (CLAUDE.md : geler les
  formules avant leurs dépendants).

## Reporté (deliberé, documenté)

- **Conservation de la main-d'œuvre** *(latent, sévérité basse)* : `connected_pop`
  peut compter la même population pour plusieurs industries (pas de partage du
  travail entre usines d'une même région) → empiler des usines autour d'un foyer
  donne une production ∝ **nombre d'usines**, pas à la population. Sans gravité en
  solo sandbox (le joueur ne triche que lui-même) **mais** à trancher avant un
  équilibrage sérieux : pool de main-d'œuvre par grappe réparti entre les usines.
  **→ Tâche d'équilibrage (E6).**
- **Boucle monétaire** : `money` était *sink-only* en E1 (aucune source) — **fermée
  en E2** (le commerce crédite l'argent). L'étalement est volontaire (tranches).
- **Double balayage 400k / itération 4-voisins dupliquée** : perf/minimalisme à
  factoriser lors de la **passe perf** (cf. `docs/REALTIME.md` Phase B).

## Non-problèmes confirmés (au crédit du code)
- Accumulations influence/matériaux indépendantes de l'ordre (sommes entières).
- Ordre des vérifs de `build()`, déduction, pas de nation fantôme, pop=0→0,
  pollution bornée (point fixe 0.2).
- `f32` d'`industry_output` acceptable ; pas d'`unwrap` en chemin de sim ; câblage
  UI correct.
