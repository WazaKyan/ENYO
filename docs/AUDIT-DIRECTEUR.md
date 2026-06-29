# Audit cruel — Directeur temps réel (version « intention »)

> Audit adversarial multi-agents (40 agents, 6 dimensions → vérification sceptique
> de chaque trouvaille → synthèse). 33 trouvailles, **32 confirmées** (2 hautes,
> 11 moyennes, 16 basses, 3 non-problèmes). Lignes citées vérifiées sur le commit.

## Verdict global

| Dimension | Verdict |
|---|---|
| **Déterminisme / rejeu** | ✅ **SAIN** — record→replay bit-identique, double-run headless identique, headless reproductible. Contrat non-négociable tenu. |
| **Audit / event-sourcing** | ⚠️ **FRAGILE** — pas de checksum d'intégrité du `.rec` ; recorder à abandon silencieux. |
| **Invisibilité** | ❌ **ÉCHEC** — métronome (tous les 3 mois pile, montant constant, **toujours** la case argmax). Telltale interdit. |
| **Concurrence** | ✅ correct par construction ; défauts : gel UI au Drop, zombie sur panic, zéro test. |
| **Équité / équilibre** | ❌ **Budget d'Équité absent** (non-négociable) ; Pression ≫ Secours. |
| **Robustesse (frontière LLM)** | ⚠️ `focus_nation` jamais validé (ghost / self / troncature). |

**Le cœur déterministe est solide. Le Directeur échoue à sa raison d'être : l'invisibilité.**

## Trouvailles confirmées (par sévérité) & correctifs

### 🔴 Hautes
- **H1 — Ciblage argmax.** Blight toujours sur la case la plus peuplée, Windfall toujours sur la plus dévastée (qui dégénère sur le coin `y=0` si dévastation uniforme), répété sur la même case. *Fix :* tirer parmi un **top-K** + **jitter pur** (graine dérivée de `world.turn`, **jamais `self.rng`**) + éviter de répéter la même case. (`ai/director.rs`, `ai/lib.rs:241-247`)
- **H2 — Métronome.** Intervention tous les `ACT_PERIOD=3` mois pile, montant figé (`intensity` constant). *Fix :* **cadence apériodique** (intervalle 2..5 jitteré) + **montant varié** (±20 % jitter). (`ai/director.rs`)

### 🟠 Moyennes
- **M1 — `focus_nation` non validé** (ghost-nation, self-grief, troncature `as u16`). Un patch ferme 6 trouvailles. *Fix :* filtrer plage + existence + `from != to` dans `parse_intent`/`resolve_tick`, **et** `reject` défense-en-profondeur dans `World::apply(DirectorGrievance)`. (`llm/lib.rs:456`, `ai/director.rs:148,175`, `sim/lib.rs:158`)
- **M2 — Fenêtre d'intention LLM ancrée au tour de requête** → « morte à l'arrivée ». *Fix :* transporter la **durée relative**, ré-ancrer `until_turn = world.turn + duration` à l'application.
- **M3 — Pas de vérif live↔rejeu ; `.rec` tronqué rejoue « OK » vers un état faux ; recorder à abandon silencieux.** *Fix :* trailer `{tour, checksum}` + assertion au rejeu + `--expect-checksum` + échec recorder fatal/HUD + golden CI.
- **M4 — Budget d'Équité absent + Pression ≫ Secours.** *Fix :* re-check d'équité par acte (abandon de stance injustifiée), compteur d'acharnement plafonné, Relief rééquilibré, cap d'accumulation du grief sous `WAR_THRESHOLD`.
- **M5 — Montant des calamités corrélé à la puissance du joueur** (régression détectable). *Fix :* `intensity` ne pilote que fréquence/ciblage, pas l'amplitude par case (recoupe H1).
- **M6 — `Drop` du worker joint le thread pendant un `curl`** → gel UI ≤ 35 s. *Fix :* `kill()` le child ou **détacher** le thread (pas de join).

### 🟡 Basses (sélection)
- **L5 — `poll()` confond canal vide/déconnecté** → zombie `in_flight` sur panic. *Fix :* distinguer `Disconnected` + `catch_unwind`.
- **L7 — Coefficients Directeur dupliqués/divergents** (`ai::direct` vs `Intent::baseline`). *Fix :* single-source des seuils.
- L1 grief sans cause organique de contact ; L3 events Directeur étiquetés (invisibilité repose sur « l'UI ne rend jamais d'events ») ; L4 harness logue le JSON brut ; L6 zéro test du Directeur temps réel ; L8 `unwrap()` hors `sim` ; L9 défauts silencieux de `parse_intent`.

### ✅ Non-problèmes (au crédit du code)
- Contrat déterminisme/rejeu **prouvé** sur tous les leviers.
- `f32` de `assess`/`dominance` **acceptable** (couche de décision, pas op spatiale, sortie entière enregistrée).
- `clippy -D warnings` propre.

## Ordre d'attaque
1. **H1 + H2** (invisibilité — justifie le Directeur ; sans ça, brancher le LLM ne corrige rien).
2. **M1** (validation `focus` — quick win, sécurise la frontière LLM).
3. **M3 + L6** (durcir l'audit + premiers tests).
4. **M4** (Budget d'Équité — restaure un non-négociable).
5. **M2** (ré-ancrage LLM).
6. **M6 + L5** (concurrence robuste).
7. Nettoyage qualité (L7, L4, L3, L9, L8).

## Angles morts à tester
- `--player` est **parsé puis jeté** (`ui` : `let _ = a.player`) → le joueur est toujours la nation 0 (souvent *struggling*) → **Pression/Blight/Grief quasi jamais exercés en headless** (démontrés par code seulement). Rendre `--player` effectif pour exercer la Pression.
- `ElevateRival` est **LLM-only** → aucune couverture déterministe.
- Tout le chemin LLM live (worker, apply-on-arrival) inobservable headless → exige un **trait `Chat` + mock**.
- Intégrité du rejeu / équivalence live↔rejeu non testées.
- `blight()` rabote la **base climatique permanente** (invraisemblable) — à traiter au passage balance.
