//! Couche LLM du Directeur (Phase 5b) : DeepSeek **choisit** parmi les leviers
//! déjà légaux et bornés du Directeur déterministe.
//!
//! Déterminisme : les décisions du LLM sont émises sous forme de [`Command`] —
//! enregistrées dans le journal, donc le replay les rejoue **sans rappeler le
//! LLM**. En cas d'échec (réseau, parsing, pas de clé), on retombe sur le
//! Directeur déterministe [`ai::direct`].

use std::io::Write;
use std::process::{Command as Proc, Stdio};

use proto::Command;
use sim::World;

/// Client DeepSeek (API compatible OpenAI).
pub struct DeepSeek {
    key: String,
    model: String,
    url: String,
}

impl DeepSeek {
    /// Construit le client depuis l'environnement / le fichier `.env`.
    /// Renvoie `None` si aucune clé n'est trouvée.
    pub fn from_env() -> Option<Self> {
        let key = read_key()?;
        let model = std::env::var("DEEPSEEK_MODEL").unwrap_or_else(|_| "deepseek-chat".to_string());
        Some(Self {
            key,
            model,
            url: "https://api.deepseek.com/chat/completions".to_string(),
        })
    }

    /// Un échange chat (délégué à `curl`) ; renvoie le contenu texte de la réponse.
    fn chat(&self, system: &str, user: &str) -> Result<String, String> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system},
                {"role": "user", "content": user},
            ],
            "stream": false,
            "temperature": 1.0,
            "response_format": {"type": "json_object"},
        })
        .to_string();

        let mut child = Proc::new("curl")
            .args([
                "-s",
                "--max-time",
                "35",
                "-X",
                "POST",
                &self.url,
                "-H",
                &format!("Authorization: Bearer {}", self.key),
                "-H",
                "Content-Type: application/json",
                "-d",
                "@-", // corps lu sur stdin (évite l'échappement en ligne de commande)
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("curl introuvable: {e}"))?;
        {
            let mut stdin = child.stdin.take().ok_or("pas de stdin curl")?;
            stdin
                .write_all(body.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out.stdout);
        let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        v["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| format!("réponse LLM sans contenu: {text}"))
    }
}

/// Lit la clé depuis la variable d'env, sinon depuis `.env` (à la racine).
fn read_key() -> Option<String> {
    if let Ok(k) = std::env::var("DEEPSEEK_API_KEY") {
        if !k.trim().is_empty() {
            return Some(k.trim().to_string());
        }
    }
    let content = std::fs::read_to_string(".env").ok()?;
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix("DEEPSEEK_API_KEY=") {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Directeur LLM : DeepSeek choisit des leviers ; fallback déterministe sinon.
pub fn direct(world: &World, player: u16, client: Option<&DeepSeek>) -> Vec<Command> {
    let client = match client {
        Some(c) => c,
        None => return ai::direct(world, player),
    };
    let (system, user) = build_prompt(world, player);
    match client.chat(&system, &user) {
        Ok(content) => match parse_actions(&content, world) {
            Some(cmds) => {
                tracing::info!(decision = %content.replace('\n', " "), "Directeur LLM");
                cmds
            }
            None => {
                tracing::warn!("réponse LLM non exploitable — fallback déterministe");
                ai::direct(world, player)
            }
        },
        Err(e) => {
            tracing::warn!(error = %e, "échec LLM — fallback déterministe");
            ai::direct(world, player)
        }
    }
}

/// Construit (system, user) : rôle du Directeur + état compact + leviers légaux.
fn build_prompt(world: &World, player: u16) -> (String, String) {
    let system = "Tu es le DIRECTEUR INVISIBLE d'un jeu de stratégie. Ton but : rendre la \
        partie la plus intéressante possible pour LE JOUEUR (difficulté constante + drama), \
        SANS qu'il ne devine jamais ton intervention. Tu n'agis que par des biais discrets \
        qui ressemblent à des événements naturels. Si le joueur domine, mets-le sous \
        pression (coalitions, calamités). S'il souffre injustement, offre-lui un répit \
        discret. Réponds UNIQUEMENT en JSON."
        .to_string();

    let mut nations = String::new();
    for n in &world.nations {
        let (pop, tiles) = world.nation_stats(n.id);
        nations.push_str(&format!(
            "{{\"id\":{},\"pop\":{:.0},\"tiles\":{}}},",
            n.id, pop, tiles
        ));
    }
    let best = player_tile(world, player, true);
    let worst = player_tile(world, player, false);

    let user = format!(
        "État (joueur = nation {player}) :\n\
         nations: [{nations}]\n\
         guerres: {wars:?}\n\
         griefs: {griev:?}\n\
         meilleure_case_joueur: {best:?}\n\
         pire_case_joueur: {worst:?}\n\
         carte: {w}x{h}\n\n\
         Choisis 0 à 3 actions parmi ces leviers LÉGAUX :\n\
         - {{\"lever\":\"grievance\",\"from\":<id_rival>,\"to\":{player},\"amount\":1..10}}\n\
         - {{\"lever\":\"blight\",\"x\":<int>,\"y\":<int>,\"amount\":1..30}}\n\
         - {{\"lever\":\"windfall\",\"x\":<int>,\"y\":<int>,\"amount\":1..40}}\n\
         Réponds en JSON: {{\"reasoning\":\"intention cachée\",\"actions\":[...]}}",
        player = player,
        nations = nations,
        wars = world.diplomacy.wars(),
        griev = world.diplomacy.grievances(),
        best = best,
        worst = worst,
        w = world.width,
        h = world.height,
    );
    (system, user)
}

/// Meilleure (plus peuplée) ou pire (plus dévastée) case du joueur.
fn player_tile(world: &World, player: u16, best: bool) -> Option<(u32, u32)> {
    let mut chosen = None;
    let mut score = -1.0f32;
    for (idx, t) in world.tiles.iter().enumerate() {
        if t.owner != Some(player) {
            continue;
        }
        let s = if best { t.population } else { t.devastation };
        if s > score {
            score = s;
            chosen = Some((idx as u32 % world.width, idx as u32 / world.width));
        }
    }
    chosen
}

/// Parse + valide la réponse JSON du LLM en commandes Directeur bornées.
/// `None` si le JSON est inexploitable (→ fallback). Cap d'équité : 3 actions.
pub fn parse_actions(content: &str, world: &World) -> Option<Vec<Command>> {
    let json = extract_json(content);
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let actions = v.get("actions")?.as_array()?;
    let mut cmds = Vec::new();
    for a in actions.iter().take(3) {
        let lever = a.get("lever").and_then(|x| x.as_str()).unwrap_or("");
        match lever {
            "grievance" => {
                let from = a.get("from").and_then(|x| x.as_u64());
                let to = a.get("to").and_then(|x| x.as_u64());
                let amount = a.get("amount").and_then(|x| x.as_u64()).unwrap_or(0);
                if let (Some(from), Some(to)) = (from, to) {
                    if from != to {
                        cmds.push(Command::DirectorGrievance {
                            from: from as u16,
                            to: to as u16,
                            amount: amount.clamp(1, 10) as u32,
                        });
                    }
                }
            }
            "blight" => {
                if let Some((x, y)) = xy(a, world) {
                    let amount = a.get("amount").and_then(|x| x.as_u64()).unwrap_or(0);
                    cmds.push(Command::DirectorBlight {
                        x,
                        y,
                        amount: amount.clamp(1, 30) as u32,
                    });
                }
            }
            "windfall" => {
                if let Some((x, y)) = xy(a, world) {
                    let amount = a.get("amount").and_then(|x| x.as_u64()).unwrap_or(0);
                    cmds.push(Command::DirectorWindfall {
                        x,
                        y,
                        amount: amount.clamp(1, 40) as u32,
                    });
                }
            }
            _ => {}
        }
    }
    Some(cmds)
}

/// Extrait des coordonnées valides (dans la carte) d'une action.
fn xy(a: &serde_json::Value, world: &World) -> Option<(u32, u32)> {
    let x = a.get("x")?.as_u64()? as u32;
    let y = a.get("y")?.as_u64()? as u32;
    if x < world.width && y < world.height {
        Some((x, y))
    } else {
        None
    }
}

/// Isole le bloc JSON (au cas où le LLM ajoute du texte autour).
fn extract_json(s: &str) -> &str {
    match (s.find('{'), s.rfind('}')) {
        (Some(a), Some(b)) if b >= a => &s[a..=b],
        _ => s,
    }
}
