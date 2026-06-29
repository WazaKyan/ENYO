# ENYO — Passage au temps réel

> Document de design. Fait autorité avec `PLAN.md` et `docs/GAMEPLAY.md`.
> En cas de divergence avec le code, ces docs priment ; toute décision de design
> qui change ici doit être répercutée dans le **même commit** que le code.
>
> Statut : **proposé**. Implémentation par tranches (voir §8). Aucune ligne de
> `sim` n'a encore changé ; ce document gèle les invariants AVANT d'écrire le code.

---

## 1. Décision & principe en une phrase

On garde une **simulation à tick fixe, 1 tick = 1 `Command::Step` = 1 mois, strictement inchangée et déterministe** ; le « temps réel » n'est qu'une **horloge murale confinée à la crate `ui`** qui décide *combien* de `Step` émettre et *quand* (pause + multiplicateurs de vitesse), sans jamais entrer dans `World` — donc le déterminisme, le rejeu et le mode headless restent **exacts par construction**.

**Ce qu'on NE fait PAS** : on rejette la subdivision sous-mensuelle (`TICKS_PER_MONTH > 1`). Elle ré-indexerait le RNG (1 tirage / `Step`, `sim/src/lib.rs:481`), forcerait à recalibrer toutes les constantes mensuelles (`GROWTH_RATE`, lerps `0.05`, `devastation *= 0.95`, `diplomacy.decay(0.99)`, `KNOWLEDGE_RATE`) et à re-bénir tous les golden replays, pour un gain **uniquement cosmétique** (fluidité), obtenable autrement (interpolation au rendu, Phase C). La sémantique du jeu est mensuelle (essaimage à 1000 pop, famine, traités, griefs, savoir) : sous-ticker créerait des artefacts de seuil.

---

## 2. Modèle de tick & boucle temps réel

### 2.1 Le tick

- **1 tick = 1 `Command::Step` = `World::resolve_turn` = 1 mois.** `World.turn: u64` (`sim/src/lib.rs:42`, déjà `pub`) reste l'unique horloge de jeu. Tout dérivé du temps en sort (`climate::month_of`, table `MONTHLY[12]`, affichage An/Mois de l'UI).
- `World::apply` (`sim/src/lib.rs:128`) reste l'**unique** porte de mutation. `Command::Step => self.resolve_turn()` inchangé.
- La fluidité visuelle, le jour où elle sera nécessaire, est le travail du **rendu** (interpolation cosmétique, Phase C), **jamais** relue par `sim`.

### 2.2 La boucle temps réel (dans `ui`, jamais dans `sim`)

Aujourd'hui le « pacing » est un **compteur de frames** couplé au 60 fps de minifb (`ui/src/main.rs:780-785`, `self.frame.is_multiple_of(12|28)` ; `set_target_fps(60)` ligne 235). On le **remplace** par un **accumulateur à horloge murale**, en microsecondes **entières** (jamais de `f32` dans le pacing) :

```text
RealtimeClock {
    last: Option<Instant>,   // None tant que le jeu n'a pas démarré
    acc_us: i64,             // accumulateur en microsecondes
    tick_period_us: i64,     // période d'un mois à la vitesse courante
    paused: bool,
}

// une fois par frame, en tête de handle_game :
let now = Instant::now();
let dt_us = last.map(|l| (now - l).as_micros() as i64).unwrap_or(0);
last = Some(now);                 // remis à CHAQUE frame, même en pause
if !paused {
    acc_us += dt_us;             // dt déjà à l'échelle de la vitesse via tick_period_us
    let mut n = 0;
    while acc_us >= tick_period_us && n < MAX_TICKS_PER_FRAME {
        self.advance();          // = un tick : end_turn() ou replay_step()
        acc_us -= tick_period_us;
        n += 1;
    }
    if acc_us > tick_period_us { acc_us = 0; } // garde anti-spirale : on JETTE le surplus
}
```

- `self.advance()` (`ui/src/main.rs:837`) dispatche déjà `replay_step()` (rejeu) vs `end_turn()` (jeu/spectateur). L'accumulateur ne décide que **combien** d'appels, pas leur contenu.
- `set_target_fps(60)` est **conservé** : il borne le rendu/CPU. La cadence de simulation, elle, dérive du vrai `dt` mesuré → **indépendante du framerate** (une machine à 30 fps simule à la même vitesse-jeu).
- **Garde anti-spirale** `MAX_TICKS_PER_FRAME` (~8) : au-delà, on jette le surplus d'accumulateur (le jeu prend du retard mural mais ne part pas en orage de ticks). C'est déterministe : seul compte le **nombre de `Step` réellement exécutés et enregistrés**.

### 2.3 Découpler du mode spectateur

Aujourd'hui l'auto-déroulé ne tourne qu'en spectateur (`if self.spectator && self.speed > 0`, ligne 780). On **retire le garde `self.spectator`** : le monde avance aussi en mode « Jouer », avec une vraie pause. `end_turn` (`ui/src/main.rs:542-545`) saute déjà la nation du joueur dans la boucle IA — le joueur garde donc le contrôle pendant que le temps s'écoule.

### 2.4 Une seule routine de tick

Extraire le corps actuel de `end_turn` (`ui/src/main.rs:533-551`) en `App::tick_once(&mut self)`. Séquence canonique **inchangée** :

```text
[commandes joueur en attente, ordre d'émission]  →  Command::Step  →  ai::direct (Directeur)  →  pour nid in 0..nations (ordre d'id croissant) : ai::plan
```

C'est exactement l'ordre producteur d'aujourd'hui, identique dans `harness` (`crates/harness/src/main.rs:122-138`) et `step_world` (`ui/src/main.rs:1229`). En Phase A mono-thread, les commandes joueur passent déjà par `App::apply` (`ui/src/main.rs:471`, l'entonnoir unique record→apply) dans l'ordre d'application : **aucun buffer n'est nécessaire** (le buffer `pending` n'arrive qu'en Phase C, cf. §8).

### 2.5 Headless inchangé

Le `harness` reste un enchaînement de `Step` **« aussi vite que possible »** (`for i in 0..args.turns`, `crates/harness/src/main.rs:122`). C'est déjà le modèle headless visé : aucune horloge, CI et golden replays tournent sans `Instant`.

---

## 3. Comment déterminisme / rejeu / headless restent EXACTS

### 3.1 Les invariants gelés

| # | Invariant | Garde |
|---|---|---|
| I1 | L'horloge murale (`Instant`, accumulateur, `paused`, `speed`) ne touche **jamais** `World`. Elle ne décide que *combien* de `Step` et *quand*. | Aucun `std::time` dans `sim` (test-garde CI : `grep`). |
| I2 | 1 tick = 1 `Command::Step` = 1 mois. Pas de payload `dt`/`tick` sur `Step`. | Doc-comment sur `Command::Step` (`proto/src/lib.rs:13`). |
| I3 | **Exactement 1** `self.rng.next_u64()` par `Step` (`sim/src/lib.rs:481`). | Test comptant les tirages / `Step`. |
| I4 | Ordre intra-tick canonique : joueur → `Step` → Directeur → nations par id. | Golden test d'ordre, `tick_once` unique. |
| I5 | Tout non-déterminisme externe (LLM) est enregistré comme **`Command` concrètes** à un **tick fixe**, jamais comme « appel à refaire ». | Contrat actuel `llm` (cf. §4). |
| I6 | Vitesse/pause n'entrent ni dans `sim` ni dans le `.rec`. | Pas de `Command::SetSpeed`/`Pause`. |

### 3.2 Pourquoi ça tient

- Le RNG `SplitMix64` (`sim/src/rng.rs`) est consommé **une fois par `Step`** pour `weather_seed`. Tant que tick == mois == 1 `Step`, la suite météo est inchangée → le `checksum` FNV-1a (`sim/src/lib.rs:602`) embarqué dans chaque `Event::TurnResolved` est **bit-identique**.
- Le `.rec` **positionnel actuel** rejoue déjà à l'identique : `Header{seed,width,height}` (`persist/src/lib.rs:21`) + une `Command` par ligne ; `replay` (`persist/src/lib.rs:76`) plie `world.apply` sur l'ordre fichier. L'accumulateur change **quand** `end_turn` est appelé, pas **ce qui** est écrit. **`sim`, `proto`, `persist` restent intacts en Phase A.**
- `World` sérialise déjà son `rng` (`sim/src/lib.rs:47`) : save/load capture l'état aléatoire, donc une reprise à une frontière de tick est déterministe (déjà prouvé par `crates/sim/tests/audit.rs`).
- Garde-fous d'ordre existants, à préserver tels quels : `BTreeSet` des frontières (`sim/src/lib.rs:544`), Dijkstra à tie-break par index (`sim/src/path.rs:71`), hash f32 en ordre d'index (`sim/src/lib.rs:606`), `Diplomacy` en `Vec` ordonné par insertion.

### 3.3 La seule nouveauté sensible : l'entrée non déterministe au mur

En temps réel, l'entrée joueur (clics) et le **retour LLM** arrivent à des instants muraux non déterministes. Deux niveaux de réponse :

1. **Phase A (recommandée d'abord)** : on **n'horodate pas**. Le `.rec` reste positionnel ; tant que les commandes passent par `App::apply` dans l'ordre d'application (mono-thread), une session live se rejoue **au bit près**. Le déterminisme est garanti **sans changer aucun format**.
2. **Phase A.1 (audit, optionnel)** : on **horodate** chaque commande enregistrée au **tick d'application** via un wrapper `proto::Timed { tick: u64, cmd: Command }` (la `Command` reste **pure**, aucun champ temporel). Le replay devient *tick-aware* et **vérifie** `timed.tick == compteur_de_Step`. Indispensable dès qu'une couche async (LLM) ou un thread (Phase C) introduit des instants muraux dans la chaîne d'enregistrement.

### 3.4 La dette à payer AVANT toute parallélisation (et seulement alors)

Les réductions `knowledge_gain[i] += dev * …` (`sim/src/lib.rs:587`) et les sommes `f64` `temp_sum`/`veg_sum` (`sim/src/lib.rs:502-503`) sont **dépendantes de l'ordre** (l'addition flottante n'est pas associative). L'accumulateur mono-thread **conserve l'ordre d'index** : il **n'exige pas** ce changement. Mais toute parallélisation `rayon` future (pour soutenir une cadence élevée) casserait silencieusement le checksum. **Prérequis non négociable avant tout `par_iter`** : passer ces réductions en **entier/fixed-point** ou en **réduction à ordre fixe**. Acté ici par écrit ; à matérialiser par un commentaire dans le code.

---

## 4. Cadence du Directeur LLM

### 4.1 Le problème

`DeepSeek::chat` (`crates/llm/src/lib.rs:36`) est un **`curl` bloquant** (`--max-time 35`, `temperature 1.0`, `response_format json_object`) appelé **inline 1×/tour** — uniquement dans le `harness` aujourd'hui (`--director-llm`, `crates/harness/src/main.rs:125-128`). L'UI n'appelle même pas le LLM (`end_turn` utilise `ai::direct`, `ui/src/main.rs:539`). Bloquer jusqu'à 35 s par tick est rédhibitoire en temps réel.

### 4.2 La solution : worker asynchrone + tick-deadline FIXE

- **Worker** `std::thread` + `std::sync::mpsc` (PAS de `tokio` : la toolchain `x86_64-pc-windows-gnu` n'en a pas besoin, on reste sur le sous-processus `curl`). Le `DeepSeek` est **déplacé** dans le thread. Types proposés (crate `llm`) :
  - `DirectorView` — agrégat **possédé** (`Send + 'static`) : `player`, `width`, `height`, `nations: Vec<(u16, f32, u32)>`, `wars`, `grievances`, `best_tile`, `worst_tile`. **Aucune référence vers `&World`** (sinon ni `Send` ni clonable à bon marché : 400k `Tile`). `DirectorView::from_world(&World, player) -> DirectorView` (1 passe).
  - `DirectorWorker` : `spawn(client: DeepSeek)`, `request(&self, view: DirectorView) -> bool` (false si une requête est déjà en vol — **un seul slot**), `poll(&self) -> Option<Result<String, String>>` (non bloquant, contenu brut), `impl Drop` (ferme le canal, join le thread).
- **Cadence** : pas par tick. Requête lancée tous les `LLM_PERIOD` **mois** (clé = `world.turn`, jamais l'horloge). Le Directeur agit déjà conceptuellement au mois.
- **Règle de déterminisme (le point dur)** : le résultat s'applique à un **tick-deadline FIXE** `T_apply = T_req + DELTA_TICKS` (`DELTA_TICKS` constant **en ticks**, ≥ l'équivalent de ~35 s de jeu), **jamais « dès que ça revient »** (qui dépendrait de la latence réseau → tick non déterministe → rejeu cassé).
- **À `T_apply`** :
  - si le worker a livré → `parse_actions(&content, world)` (déjà bornée à 3 actions, `crates/llm/src/lib.rs:194`), appliquer + **enregistrer** ses `Command` Directeur ;
  - sinon → fallback déterministe `ai::direct(world, player)` (déjà le contrat, `crates/llm/src/lib.rs:108`), appliquer + enregistrer.
  - Dans **les deux cas**, on enregistre des `Command` **concrètes** au tick `T_apply` ⇒ le replay **ne rappelle jamais DeepSeek** (contrat actuel, étendu au tick). Le choix LLM-vs-fallback est capturé *de facto* par les commandes enregistrées.
- **Réponse tardive** : si elle arrive après `T_apply` (à x4/Max/headless, `T_apply` précède presque toujours le réseau), elle est **jetée** (le Directeur du tick est marqué résolu). `Drop` du worker au quit / nouvelle partie / chargement de replay pour qu'aucune réponse n'atterrisse sur le mauvais `World`.
- **Harness inchangé** : il garde l'appel **synchrone** inline (pas de budget de frame ; bloquer ≤ 35 s en batch est correct et laisse réellement le LLM répondre). Les commandes Directeur y sont enregistrées au tick courant → un `.rec` produit par le harness et un `.rec` capturé par l'UI rejouent à l'identique.

### 4.3 Cadence de l'IA déterministe

`ai::direct` et `ai::plan` sont purs mais coûteux (scans pleine grille 400k, `O(N×400k)`). À haute vitesse, on les **cadence sur `world.turn`** (jamais le mur) : `directs_this_tick(turn)`, `plans_this_tick(turn, nation)` (étalement `turn % AI_PLAN_PERIOD == nation % AI_PLAN_PERIOD`). **Changer qui agit à quel tick modifie le flux enregistré → re-bénir les goldens une fois** (coût ponctuel), déterminisme strictement préservé car la clé reste `world.turn`.

---

## 5. Pause & vitesses, mapping 1 mois = X

- **100 % dans `ui`**, jamais dans `sim` ni dans le `.rec`. Le champ existant `speed: u32` est réinterprété en **multiplicateur** ; `tick_period_us = BASE_TICK_US / mult`.
- `enum Speed { Pause, X1, X2, X4, Max }` (ou le chemin `u32` direct, plus minimal, qui réutilise `GameBtn::Speed`) :
  - **Pause** = on n'accumule pas (rendu et caméra continuent). `last = now` est **quand même** remis à chaque frame, sinon le dépaussage déverse une rafale de `Step`.
  - **X1/X2/X4** = `tick_period_us = BASE_TICK_US / mult`.
  - **Max** = on débraye l'accumulateur (rafale bornée par `MAX_TICKS_PER_FRAME`), pour le fast-forward / headless.
- **Mapping proposé** (knob `BASE_TICK_US`, vit uniquement dans `ui`, jamais lu par `sim` ; au pire documenté dans `Header`, jamais lu au replay) :

| Vitesse | Période / mois | Repère |
|---|---|---|
| Pause | ∞ | — |
| **X1** | **500 ms** | 1 an de jeu ≈ 6 s ; 1 siècle ≈ 10 min |
| X2 | 250 ms | 1 siècle ≈ 5 min |
| X4 | 125 ms | 1 siècle ≈ 2,5 min |
| Max | ASAP | perf-bound (siècles en secondes) |

- Cohérent avec la calibration **inchangée** : croissance logistique 8 %/mois (`dynamics.rs:39`), `tech_cost = 25*(tier+1)` (`sim/src/lib.rs:650`). Une partie longue (2400 mois / 200 ans) ≈ 20 min à X1.
- **La vitesse n'agit QUE sur `tick_period_us`.** Elle ne touche **aucune** constante mensuelle : « x2 » ne double pas `GROWTH_RATE`, sinon un mois ne vaut plus un mois et le rejeu à cadence différente devient impossible.
- **Exposition UI** : sortir le cluster vitesse de `if self.spectator` (`ui/src/main.rs:987-992`) pour le rendre visible dans tous les modes, placé dans la barre du haut (à côté de `EndTurn`/`Menu`). Garder **`Espace` = un tick manuel** (l'`Auditor` l'utilise, `crates/harness`/`ui` ; ne pas le casser) et le bouton « Fin de tour » = un tick manuel (utile en pause). Les touches numériques 1-4 restent la **recherche** en mode Jeu (collision réelle : `ui/src/main.rs:756-763`).
- **Dégradation gracieuse** : sous charge, la garde anti-spirale fait que la vitesse réelle plafonne sous la consigne. Exposer « vitesse réelle vs consigne » en UX. C'est déterministe (seul le nombre de `Step` exécutés compte).

---

## 6. Refactors par crate

> Convention : **cassant** = casse la compilation des appelants OU invalide les golden checksums (re-bless intentionnel). **Effort** : S < M < L.

| Crate | Changements clés | Cassant ? | Effort |
|---|---|---|---|
| `proto` | Phase A : **aucun** (gel des enums). Phase A.1 : `Timed{tick,cmd}` additif + doc-comment d'invariant sur `Step`. Interdire `Command::Pause/SetSpeed`. | Non | S |
| `sim` | Phase A : **aucun** (cœur intact). Phase B : `set_fast`/`is_fast` (saut de checksum en live, `#[serde(skip)]`, défaut false) ; `PathScratch` + `reach_cost_into` (tuer l'alloc 400k/Swarm) ; `pop_scratch` réutilisé ; `all_nation_stats()` one-pass partagé. | Non | S |
| `ai` | Phase A : **aucun**. Phase B : `plans_this_tick`/`directs_this_tick` (cadence clée `world.turn`) ; `assess` one-pass ; exposer `DirectorView` + `decide` (séparation lecture/décision). | **Oui** (re-bless goldens si cadence) | M |
| `llm` | `DirectorView` + `from_world` ; `DirectorWorker` (thread + mpsc, possède `DeepSeek`) ; `build_prompt(&DirectorView)` ; `direct(...)` réimplémenté par-dessus (signature conservée) ; `resolve_reply` (règle de fallback). | Non | M |
| `persist` | Phase A : **aucun**. Phase A.1 : `Header{version, ticks_per_month}` (`#[serde(default)]`) + `Header::new` ; `Timed` ; `Recorder::record_at(tick, cmd)` ; `read_recording_timed` / `replay_timed` (assert tick==compteur Step). | **Oui** (litéraux `Header`, format) | M |
| `render` | Phase A : **aucun** (déjà découplé des ticks, consommateur par-frame). Phase B (opt.) : `viewport_into(&mut [u32])` (zéro alloc/copie par frame). Phase C (opt.) : `region_interp`/`viewport_interp_into` (interpolation cosmétique). | Non | S |
| `ui` | `RealtimeClock` (accumulateur µs) en remplacement du compteur de frames ; `tick_once` ; vitesse en multiplicateur + boutons tous-modes ; throttle `recompute_stats` (~10 Hz) ; Phase B : worker LLM ; Phase C : `pending: Vec<Command>`. | Non | M |
| `harness` | Boucle batch **inchangée** (référence ASAP). Phase A.1 : `record_at(tick)` ; `Header::new` ; `run_replay` *tick-aware* ; flag `--expect-checksum <u64>` + `tests/golden.rs`. LLM **reste synchrone inline**. | **Oui** (signature `record`, `Header`) | M |

### 6.1 Détails notables (types & fonctions)

- **`sim::path::PathScratch { dist: Vec<u32>, stamp: Vec<u32>, gen: u32 }`** + `reach_cost_into(&mut PathScratch, …)` : reset **O(1)** par compteur de génération (`dist[i]` valide ssi `stamp[i] == gen`). `reach_cost` (`sim/src/path.rs:54`) devient un **wrapper** (non cassant). Le `dist = vec![u32::MAX; 400000]` alloué **par appel** aujourd'hui (`sim/src/path.rs:66`) est le **pire facteur d'échelle** : `ai::expansion` (`crates/ai/src/lib.rs:76`) émet un `Swarm` par case frontière ≥ 1000 hab → des dizaines à centaines d'appels/tour en fin de partie. Scratch stocké en `World.path_scratch` (`#[serde(skip)]`).
- **`World.fast_no_checksum: bool`** (`#[serde(skip)]`, défaut **false** = checksum actif, pour éviter le piège serde du bool) : dans `resolve_turn` (`sim/src/lib.rs:516`), `let checksum = if self.fast_no_checksum { 0 } else { self.checksum() };`. Le checksum (~20 M itérations FNV/`Step`) est le plus gros coût **fixe** ; le `.rec` ne stocke que des `Command` (le checksum vit dans l'`Event`, jamais persisté) → le **replay recalcule et vérifie** normalement.
- **`sim::all_nation_stats() -> Vec<(f32, u32)>`** (one-pass) : tue le `O(N×400k)` de `assess` (`crates/ai/src/lib.rs:210`, qui appelle `nation_stats` par nation, chacun rescannant 400k). Partagé par `ai`, `llm::build_prompt` (`crates/llm/src/lib.rs:141`) et `ui::recompute_stats` (`ui/src/main.rs:636`), qui refont aujourd'hui le même re-scan. **Garder l'ordre d'index** (somme f32) pour ne pas faire basculer les seuils `DOMINANCE_PRESSURE 0.10` / `DOMINANCE_BLIGHT 0.25`.
- **`#[serde(skip)]` obligatoire** sur tous les scratch (`path_scratch`, `pop_scratch`, `fast_no_checksum`) : sinon les snapshots gonflent et `crates/sim/tests/audit.rs` (roundtrip) diverge. C'est le seul moyen par lequel ces refactors perf pourraient casser le contrat d'audit.

---

## 7. Pièges déterminisme & mitigations

> Classés par sévérité. Les pièges « haute » sont des ruptures de rejeu ; ils doivent être couverts par des tests.

### Sévérité haute

1. **Mutation du monde live hors de `App::apply`.** `step_world` (`ui/src/main.rs:1229`) et `ensure_menu_world` (`ui/src/main.rs:623`) appellent `world.apply` **sans recorder**. → L'accumulateur ne doit appeler **que** `advance() → tick_once() → App::apply`. `step_world`/`ensure_menu_world` restent réservés au **monde jetable du menu**. Test : `nb de Step enregistrés == world.turn` en fin de session.
2. **Cadence dérivée du framerate.** Conserver `self.frame.is_multiple_of(…)` ferait du framerate une **entrée** de la cadence. → Accumulateur en µs **entières** mesurant `Instant::now()` ; retirer `self.frame` du chemin d'avance.
3. **Résultat LLM appliqué « dès qu'il revient ».** La latence réseau est non déterministe. → Épingler à `T_apply = T_req + DELTA_TICKS` (constante **en ticks**) ; exactement **une** source (LLM **ou** fallback) appliquée et enregistrée.
4. **Replay qui rappelle DeepSeek.** Journaliser une « intention d'appel » rejouerait un modèle non déterministe. → Ne **jamais** journaliser l'appel ; n'enregistrer que les `Command` concrètes (déjà bornées par `parse_actions`) **ou** le fallback.
5. **Tirage RNG supplémentaire par tick.** Toute nouvelle alea inline (événement « aléatoire », jitter, tie-break stochastique) ré-indexe `SplitMix64` → météo de tous les tours suivants changée. → Geler « 1 tirage / `Step` » ; toute alea nouvelle dérive un sous-flux seedé (`SplitMix(weather_seed ^ role_const)`) sans toucher `self.rng`, et si externe, enregistrée. Test comptant `next_u64` / `Step`.
6. **Snapshot pris pendant une requête LLM en vol.** `save_snapshot` ne sérialise que `World` (`persist/src/lib.rs:86`) ; la requête pendante + sa règle de fallback vivent hors `World`. → Confiner save/load aux **frontières de tick sans requête en vol** (règle d'appelant, documentée). Si un jour il faut sauver en vol : sérialiser le descripteur de requête + le fallback dans `ai`/`llm`, **jamais** dans `persist`/`World`.
7. **Équivalence session-live ↔ replay headless non testée.** `run_replay` (`crates/harness/src/main.rs:326`) ne compare un replay qu'à lui-même. → Ajouter un test qui **scripte une session live** (l'`Auditor` pilote déjà `App` via des `Input` scriptés), enregistre, puis rejoue le `.rec` produit et **asserte le même checksum**. Côté harness : `--expect-checksum` + golden figé.
8. **Partage de `&World`/`Arc<World>` avec le worker LLM.** Lectures déchirées / data race. → Le main thread calcule l'agrégat (1 passe, jamais les 400k brutes) et **move** un `DirectorView` 100 % possédé. Les canaux mpsc ne transportent que du possédé.

### Sévérité moyenne

9. **Parallélisation des réductions f32/f64 à ordre dépendant** (`knowledge_gain` `sim:587`, `temp_sum`/`veg_sum` `sim:502-503`). → Interdire `par_iter` sur ces passes tant qu'elles ne sont pas en entier/fixed-point ou à ordre fixe. L'accumulateur mono-thread ne l'exige pas.
10. **Ordre intra-tick dupliqué** (3 copies : `end_turn`, boucle `harness`, `step_world`). → Un seul `tick_once` ; golden test d'ordre ; toute modif = re-bless intentionnel.
11. **Multiplicateur de vitesse implémenté en modifiant la math du tick.** = la subdivision sous-mensuelle déguisée. → La vitesse n'agit que sur `tick_period_us`. Math par tick **invariante**.
12. **Pause qui continue d'accumuler.** `last` non remis à `now` en pause → rafale au dépaussage. → `last = now` chaque frame ; n'accumuler que si `!paused` ; `last: Option<Instant>` initialisé à l'entrée en jeu.
13. **Cadence IA/Directeur clée sur `self.frame` ou `Instant`** au lieu de `world.turn`. → Clé **exclusivement** `world.turn` ; `P` fixe documenté.
14. **Scratch buffers mal isolés** (reset par génération bogué → `dist[]` périmé → `Swarm` accepté/refusé différemment). → Tous en `#[serde(skip)]` + `Default` au load ; test : `reach_cost_into` (scratch réutilisé) == `reach_cost` (alloc fraîche).
15. **Input joueur appliqué immédiatement sur sim multi-thread (Phase C).** En mono-thread c'est replay-safe. → Prérequis du threading : `pending: Vec<Command>` vidé à la frontière de tick avant `Step`, en ordre canonique. **Pas nécessaire en Phase A.**
16. **`.rec` mêlant `Command` nues (legacy) et `Timed`.** → Gater par `Header.version` (`#[serde(default)]`) ; `read_recording` (legacy) vs `read_recording_timed` ; jamais de mélange. `Timed.tick` = compteur de `Step`, **jamais** l'horloge murale.

### Sévérité basse

17. **`HashSet targeted` dans `ai::expansion`** (`crates/ai/src/lib.rs:80`) : sain aujourd'hui (membership only, jamais itéré). → Le garder membership-only, ou `BTreeSet` s'il doit un jour être itéré.
18. **Échec d'écriture du recorder silencieux.** `App::apply` (`ui/src/main.rs:473-481`) drop le recorder sur erreur et continue de muter le monde. → Surfacer l'erreur (HUD + log), voire pauser ; garantir (et tester) que le préfixe `.rec` rejoue jusqu'à la troncature.
19. **Interpolation de rendu (Phase C) réinjectée dans `World`** ou `alpha` lu par la sim. → Interpolation **render-only** : lit deux snapshots, écrit le framebuffer, interpole **uniquement** les champs continus (`population`, `development`, `devastation`, `temperature`, `vegetation`), **snappe** `owner`/`kind`/`biome`/frontières sur `next` ; `alpha` jamais relu par `sim`.
20. **Recorder actif par erreur en replay.** Invariant : `replay_mode ⇒ recorder == None` (déjà vrai, `ui/src/main.rs:563`). → L'accumulateur peut piloter `advance()` en replay (lecture seule) mais ne doit jamais enregistrer. Test : un replay auto-déroulé n'écrit aucun `.rec`.
21. **Parties très longues (2400+ mois).** `temperature` non bornée (`climate.rs:30`) ; risque accru de NaN propagé par `to_bits()`. → Étendre l'invariant de finitude (`crates/sim/tests/audit.rs`) sur de longues durées en CI ; envisager un clamp de température. Rappel : le contrat est le rejeu **intra-machine**.

---

## 8. Roadmap phasée (tranches verticales testables)

> Chaque phase est **shippable seule** et close par des tests. On suit l'ordre des priorités : déterminisme/rejeu/headless > perf 400k > minimalisme > LLM. Cette roadmap se branche après la Phase 7a de `PLAN.md` (renderer headless), dans le sillage de la Phase 7b (UI interactive).

### Phase A — Horloge murale mono-thread (cœur du temps réel)

- **Périmètre** : `ui` uniquement. `RealtimeClock` (accumulateur µs) remplace le compteur de frames ; `tick_once` extrait ; vitesse en multiplicateur ; pause réelle ; monde qui avance en mode Jeu ; vitesse découplée de `spectator`.
- **Invariant** : `sim`/`proto`/`persist` **inchangés**. Le `.rec` positionnel rejoue tel quel.
- **Tests / golden** :
  - Une **session live scriptée** (via l'`Auditor` `Input`), enregistrée, puis `persist::replay` du `.rec` → **même checksum** (équivalence live ↔ replay).
  - Deux sessions à **vitesses différentes** (X1 vs X4) sur le même script → **`.rec` et checksums identiques** (la vitesse ne touche pas le contenu).
  - `harness --turns N` inchangé (golden checksum existant vert).
- **Shippable** : oui — auto-défilement temps réel, pause, x1/x2/x4, déterminisme prouvé.

### Phase A.1 — Horodatage & rejeu *tick-aware* (audit, optionnel mais recommandé avant async/thread)

- **Périmètre** : `proto::Timed`, `persist::{Header::new, FORMAT_VERSION, Timed, record_at, read_recording_timed, replay_timed}`, `harness` (`record_at`, `run_replay` tick-aware, `--expect-checksum`, `tests/golden.rs`).
- **Tests / golden** : `replay_timed` asserte `timed.tick == compteur_de_Step` ; relecture des **anciens `.rec` positionnels** via `Header.version` (`#[serde(default)]`) ; roundtrip `Header` (`Eq` conservé).
- **Shippable** : oui — audit renforcé, rétrocompatible.

### Phase B — Perf 400k & cadence (avant de monter la vitesse)

- **Périmètre** : `sim` (`PathScratch`/`reach_cost_into`, `pop_scratch`, `set_fast`, `all_nation_stats`) ; `ai` (`plans_this_tick`/`directs_this_tick`, `assess` one-pass) ; `render` (`viewport_into`, optionnel).
- **Tests / golden** : `reach_cost_into` == `reach_cost` (scratch réutilisé) ; checksum **inchangé** pour les optimisations pures (scratch, one-pass à ordre d'index) ; **re-bless unique** pour la cadence IA (changement de comportement intentionnel) ; `viewport_into(buf) == viewport_argb(...)`.
- **Shippable** : oui — X4/Max soutenus sur carte remplie ; `set_fast(true)` pendant le défilement rapide (checksum recalculé au replay).

### Phase B.2 — Directeur LLM asynchrone

- **Périmètre** : `llm` (`DirectorView`, `DirectorWorker`, `resolve_reply`) ; `ui` (champs `director_rx`, `next_request_tick`, `director_deadline`, `poll_director`). Harness **inchangé** (synchrone).
- **Tests / golden** : worker mocké → session live avec `T_apply` fixe → rejeu **identique** ; fallback `ai::direct` enregistré quand pas de réponse à `T_apply` ; `Drop` propre (quit / nouvelle partie / load replay) ; aucune réponse tardive appliquée.
- **Shippable** : oui — Directeur LLM en temps réel sans bloquer la boucle, rejeu exact sans réseau.

### Phase C — Promotion thread + interpolation (seulement si un `Step` dépasse ~16 ms)

- **Périmètre** : thread `sim` + `RenderSnapshot` léger (triple-buffer / `ArcSwap`) + interpolation cosmétique (`render::region_interp`) ; `ui` (`pending: Vec<Command>` vidé à la frontière de tick).
- **Invariant clé** : la **frontière de déterminisme est identique** à la Phase A (`World` ne voit que `Step` + `Timed`), donc **promotion sans changer ni `World` ni le format `.rec`**.
- **Tests / golden** : ordre intra-tick (joueur bufferisé → `Step` → Directeur → IA) figé par golden ; pas de tearing ; interpolation render-only (pas de réinjection dans `World`).
- **Shippable** : oui — 60 fps fluide même quand un `Step` déborde son budget. **À ne déclencher que sur preuve de profilage** (YAGNI sinon).

---

## 9. Questions ouvertes pour l'humain

1. **Mapping de vitesse** : `BASE_TICK_US` = 500 ms/mois à X1 est-il le bon ressenti ? Faut-il une vitesse X8, une vitesse « lente » X0.5 pour observer ?
2. **Latence LLM tolérée** : `DELTA_TICKS` (en mois) et `LLM_PERIOD` (requête tous les K mois) ? Plus `DELTA_TICKS` est grand, plus le LLM « atterrit » souvent à basse vitesse ; plus il est petit, plus le fallback déterministe domine.
3. **Étalement IA** : accepte-t-on le **re-bless unique** des goldens qu'imposent `AI_PLAN_PERIOD`/`DIRECTOR_PERIOD` ? Sinon on garde l'IA à chaque tick (coûteux à X4+).
4. **Horodatage dès la Phase A ou rester positionnel ?** Recommandation : positionnel d'abord (zéro dette), `Timed` en A.1 quand l'async/thread l'exige.
5. **Mode « Jouer » au démarrage** : le monde démarre-t-il **en pause** (recommandé, comme `start_game` met `speed=0` pour le joueur) ou à X1 ?
6. **`set_fast` (checksum sauté en live)** : acceptable que l'audit repose sur le **recalcul au replay** plutôt que sur le checksum live à haute vitesse ? (Le contrat golden reste sur le replay.)
7. **Interpolation au rendu (Phase C)** : vaut-elle son coût (double-buffer ~25-30 Mo, gestion du popping owner/frontières) à 1 mois/tick, ou un simple crossfade de la couche couleur suffit-il ?
8. **Parties très longues** : faut-il **clamper la température** (`climate.rs:30`) pour blinder la finitude sur 2400+ mois, au risque de modifier le comportement (et re-bless) ?
9. **Où afficher « vitesse réelle vs consigne »** quand la garde anti-spirale plafonne la cadence sous charge ?

---

*Fin du document. Toute modification d'un invariant (§3.1) ou du mapping de vitesse (§5) doit être répercutée dans `PLAN.md`/`docs/GAMEPLAY.md` dans le même commit.*
