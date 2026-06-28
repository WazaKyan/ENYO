# ENYO

Jeu de stratégie **minimaliste** à l'échelle d'un monde, en Rust. Le joueur fait croître
et essaimer sa civilisation sous l'œil d'une IA « Directeur » invisible.

- Design & décisions : [`PLAN.md`](PLAN.md)
- Systèmes de gameplay : [`docs/GAMEPLAY.md`](docs/GAMEPLAY.md)
- Principes de développement : [`CLAUDE.md`](CLAUDE.md)

## Voir le jeu (le plus simple)

Double-clique **`run.bat`** : il compile, lance une partie (8 nations, Directeur)
et ouvre les images générées dans `out/` :
- `monde.png` — la carte du monde (biomes, reliefs, océans) ;
- `civilisations.png` — zoom sur les nations (villes, frontières, guerres) ;
- `tileset.png` — les tuiles pixel-art.

## Prérequis

- **Rust** (toolchain stable), installé via [rustup](https://rustup.rs).

## Commandes

```sh
cargo build                                   # construire
cargo test                                    # tests (déterminisme / replay)
cargo run -p harness -- --seed 42 --turns 100 # lancer la simulation
```

Le harness écrit un journal d'événements **JSONL** (un événement par ligne) dans
`logs/run.jsonl` — auditable et rejouable.

## Structure

| Crate | Rôle |
|---|---|
| `crates/proto` | Commandes & événements : le « langage » du jeu |
| `crates/sim` | Cœur de simulation : pur, déterministe, headless |
| `crates/harness` | CLI pour piloter et tester la simulation |
