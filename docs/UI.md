# ENYO — Phase 7b : interface jouable (décision issue du fan-out)

> Décidé via fan-out multi-agents (6 pistes explorées + critiquées, build **testé
> empiriquement** sur la toolchain GNU réelle).

## Moteur : `minifb` (fenêtre framebuffer native)

- **Build GNU prouvé** : `minifb = "=0.28.0"` compile **et lie** sur la toolchain
  (winapi → `winapi-x86_64-pc-windows-gnu`, libs d'import **précompilées**,
  **zéro `windows-sys`/raw-dylib**). winit/softbuffer/wgpu échouent sur `dlltool`
  (raw-dylib + `as`/`gcc` absents).
- **Réutilise** `render::region()` : la fenêtre blitte le `RgbImage` (→ `Vec<u32>`
  `0x00RRGGBB`). `sim` intouché ; la fenêtre n'émet que des `proto::Command`
  (même pipeline que le harness) → **déterminisme intact**.
- **Plan B** : `tiny_http` (pur Rust, build prouvé) sert `render::region()` en PNG
  sur `127.0.0.1` → page web. Mode d'échec indépendant ; même image que l'agent.

## Garde-fous build (go/no-go CI)
- Épingler `minifb = "=0.28.0"` + committer `Cargo.lock`.
- **Canari** : `cargo tree -p ui` DOIT contenir `winapi-x86_64-pc-windows-gnu` et
  NE JAMAIS contenir `windows-sys` / `windows-link` / `windows-targets`.
- Garder `gcc`/`as` **hors PATH** (pour qu'un C accidentel échoue bruyamment).
- HUD : **bitmap-font maison** 5×7 (0 dépendance) au début ; `fontdue` (pur Rust) plus tard.

## Interface (crate `ui`, consommateur de sim+render+proto)
Fenêtre 1280×720, upscale nearest (pixel-art).
- **Centre** : viewport carte = `render::region(world, cam_x, cam_y, cols, rows, px)`,
  **pan** (drag / WASD / flèches), **zoom** molette (px ∈ {8,12,16,20,24,32}).
- **Bas** : barre de tour — **[Fin de tour ▸] = `Step`**, mois/année (1 tour = 1 mois),
  vitesse spectateur Pause/×1/×2.
- **Gauche** : panneau nation du joueur (pop, provinces, savoir, 4 paliers de tech, guerres).
- **Droite** : inspecteur de case au clic (terrain, capacité, pop, dev, dévastation, force, owner).
- **Modales** Tech / Diplo / Militaire (raccourcis clavier).
- **Directeur INVISIBLE** : ses effets n'apparaissent que comme événements organiques
  (un overlay `hidden_intent` réservé à l'agent/audit, derrière un flag, jamais pour l'humain).

## Mapping UI → Commande
- `[Fin de tour]` / Espace → `Step` (puis Directeur + IA, comme le harness)
- Outil **Fonder** (S) + clic terre → `Settle{ x, y, nation: player, 300 }`
- Outil **Essaimer** (E) + clic source→cible → `Swarm{ from, to }`
- Outil **Mobiliser** (M) + clic (montant molette) → `Mobilize{ x, y, player, amount }`
- Outil **Marcher** (A) + clic source→case adjacente → `March{ from, to }`
- Panneau Tech (1–4) → `Research{ player, branch }`
- Panneau Diplo → `DeclareWar` / `MakePeace`
- Directeur (`DirectorGrievance/Blight/Windfall`) : **jamais exposé au joueur**.

## Visibilité agent (PNG)
Une **unique** fonction `frame(world, viewport) -> RgbImage` (= `render::region`)
sert la fenêtre **et** le dump PNG → pixel fenêtre == pixel PNG. Mode
`ui --headless --turn N --cam x,y --px P --shot f.png` rejoue la même caméra/tour.
Les `--png/--region/--gif` du harness restent dispo. Replay depuis `.rec.jsonl`.

## MVP (phasé)
- **A** — crate `ui` (sim+render+proto), `minifb` épinglé, fenêtre 1280×720 blittant
  `render::region()` ; valider build + canari `cargo tree` ; `run-jeu.bat`.
- **B** — viewport pan/zoom recalculant `region()` ; `[Fin de tour]=Step` via un
  `run_command` partagé (rec+log) ; mois/année.
- **C** — inspecteur de case au clic ; outils Fonder/Essaimer (2 clics) ; HUD bitmap.
- **D** — panneau nation ; Research/Mobilize/March ; Diplo (war/peace) ; vitesse.
- **E** — `frame()` factorisée fenêtre+PNG ; `--headless --shot` ; Directeur dans la
  boucle Step ; cache du viewport (re-rendu si `turn` ou caméra change).

## Décisions ouvertes (humain)
- Livrer **aussi** la voie web (tiny_http) dès le départ, ou minifb seul ?
- Overlay debug Directeur réservé à l'agent (derrière flag) — OK ?

## Réalisé (A→C + menu/GUI)

- **Fenêtre ajustée à l'écran** : fenêtré ~90 % de l'écran (taille via Win32
  `GetSystemMetrics`, dep `winapi` — libs d'import précompilées, build GNU OK) ;
  **plein écran** sans bordure (bascule dans Paramètres → recrée la fenêtre).
- **GUI maison** (`crates/ui/src/gui.rs`) dessinée dans le framebuffer : police
  bitmap 8×8 (dep `font8x8`, données const pures), `Canvas` (rect/voile/texte),
  `Button` (survol + état actif). Repli ASCII des accents. **Plus de HUD dans la
  barre de titre.**
- **Machine à états** : Menu (Jouer / Spectateur / Paramètres / Quitter) →
  Paramètres (graine, nations, zoom, plein écran) → Jeu. Fond du menu = aperçu
  du monde assombri (mis en cache par taille).
- **Jeu** : carte plein cadre + barre haut (An/Mois, stats nation, Fin de tour,
  Menu) + barre bas (outils Inspecter/Fonder/Essaimer, recherche E/T/F/L ou
  vitesse Pause/×1/×2 en spectateur) + panneau d'inspection au clic + message
  d'action (succès vert / **REJET** rouge — rien n'est silencieux côté joueur).
  Souris **et** clavier (WASD, molette, Espace, F/E/N, 1-4, Échap).
- **Visibilité agent** : `render::save_argb()` + `ui --headless --screen
  menu|settings|game --shot f.png` rend exactement l'écran en PNG → chaque écran
  est vérifiable sans ouvrir la fenêtre.

### Reste (D/E)
- Outils **militaires** (Mobiliser/Marcher) et **diplomatie** (guerre/paix) pour
  le joueur (actuellement IA seule).
- **Enregistrement** des commandes UI dans un `.rec.jsonl` (replay du jeu joué).
- Équilibrage des vitesses croissance/recherche.
