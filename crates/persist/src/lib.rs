//! Persistance & audit : enregistrement **rejouable** des commandes, snapshots,
//! et replay vérifié.
//!
//! Principe : les **commandes** sont les ENTRÉES de la simulation. Avec le seed et
//! les dimensions (l'[`Header`]), elles suffisent à reconstruire la partie à
//! l'identique (event-sourcing). Les *événements* (sorties) portent des checksums
//! qui permettent de vérifier que le replay reproduit bien l'état.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use proto::{Command, Event};
use serde::{Deserialize, Serialize};
use sim::World;

/// Type d'erreur unifié (I/O + (dé)sérialisation).
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// En-tête d'un enregistrement : tout ce qu'il faut pour recréer le monde initial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Header {
    pub seed: u64,
    pub width: u32,
    pub height: u32,
}

/// Enregistreur de commandes au format JSONL : 1re ligne = en-tête, puis une
/// commande par ligne. Les commandes seules (re)produisent toute la partie.
pub struct Recorder {
    file: File,
}

impl Recorder {
    /// Crée un enregistrement et écrit l'en-tête.
    pub fn create(path: impl AsRef<Path>, header: &Header) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut file = File::create(path)?;
        writeln!(file, "{}", serde_json::to_string(header)?)?;
        Ok(Self { file })
    }

    /// Ajoute une commande à l'enregistrement.
    pub fn record(&mut self, command: &Command) -> Result<()> {
        writeln!(self.file, "{}", serde_json::to_string(command)?)?;
        Ok(())
    }
}

/// Lit un enregistrement : en-tête + liste de commandes.
pub fn read_recording(path: impl AsRef<Path>) -> Result<(Header, Vec<Command>)> {
    let file = File::open(path)?;
    let mut lines = BufReader::new(file).lines();
    let header_line = match lines.next() {
        Some(line) => line?,
        None => return Err("enregistrement vide".into()),
    };
    let header: Header = serde_json::from_str(&header_line)?;
    let mut commands = Vec::new();
    for line in lines {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        commands.push(serde_json::from_str(&line)?);
    }
    Ok((header, commands))
}

/// Rejoue un enregistrement : reconstruit le monde et applique les commandes.
/// Renvoie le monde final et tous les événements produits.
pub fn replay(header: &Header, commands: &[Command]) -> (World, Vec<Event>) {
    let mut world = World::new(header.seed, header.width, header.height);
    let mut events = Vec::new();
    for cmd in commands {
        events.extend(world.apply(cmd.clone()));
    }
    (world, events)
}

/// Sauvegarde un snapshot complet du monde (JSON).
pub fn save_snapshot(world: &World, path: impl AsRef<Path>) -> Result<()> {
    if let Some(parent) = path.as_ref().parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, serde_json::to_string(world)?)?;
    Ok(())
}

/// Charge un snapshot complet.
pub fn load_snapshot(path: impl AsRef<Path>) -> Result<World> {
    let json = std::fs::read_to_string(path)?;
    let mut world: World = serde_json::from_str(&json)?;
    // L'index des cases possédées n'est pas sérialisé (dérivé) → le reconstruire.
    world.rebuild_owned_index();
    Ok(world)
}
