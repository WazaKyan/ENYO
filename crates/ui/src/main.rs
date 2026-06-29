//! Fenêtre de jeu ENYO (minifb) — menu, paramètres, jeu tour par tour.
//!
//! Interface dessinée dans le framebuffer (boutons + HUD, police bitmap `gui`).
//! La fenêtre s'ajuste à l'écran (fenêtré ~90 % ou plein écran sans bordure).
//!
//! TEMPS RÉEL : le monde avance seul selon la vitesse (Pause / ×1 / ×2 / ×4 /
//! Max, boutons en bas). L'horloge murale est confinée ici (`RealtimeClock`),
//! `sim` ne la voit jamais → déterminisme/rejeu intacts (1 tick = 1 mois).
//! Espace = lecture / pause. clic = inspecter / agir · E = Étendre (2 clics) ·
//! V = Ville · M = Unités · G = Guerre · P = Paix (clic sur case ennemie) ·
//! N = inspecter · 1-4 = recherche (mode Jeu) · WASD = bouger · molette = zoom ·
//! Échap = menu.
//! En spectateur / rejeu : 0-4 = vitesse au clavier.
//!
//! Mode agent : `--headless --shot f.png [--screen menu|settings|game]`
//! rend exactement l'écran demandé en PNG (vérifiable sans ouvrir la fenêtre).

mod gui;

use std::time::Instant;

use gui::{Button, Canvas};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use proto::{Building, Command, Event, UnitKind};
use sim::World;

const TOP_H: i32 = 40;
const BOT_H: i32 = 126; // 3 rangées : actions ; bâtir ; recherche + vitesse

/// Période d'un mois à la vitesse x1 (horloge murale, microsecondes entières).
const BASE_TICK_US: i64 = 500_000;
/// Garde anti-spirale : ticks max résolus en une frame (sinon orage de Step).
const MAX_TICKS_PER_FRAME: u32 = 8;
/// Intervalle mural minimal entre deux appels au Directeur LLM (latence + coût).
/// Le rejeu ne dépend pas du mur : seuls les leviers concrets émis sont enregistrés.
const LLM_MIN_SECS: u64 = 18;

/// Période (µs) d'un tick selon la vitesse ; 0 = pause/max (gérés à part).
fn tick_period_us(speed: u32) -> i64 {
    match speed {
        1 => BASE_TICK_US,
        2 => BASE_TICK_US / 2,
        3 => BASE_TICK_US / 4,
        _ => 0,
    }
}

/// Libellé court d'une vitesse, pour le HUD.
fn speed_label(speed: u32) -> &'static str {
    match speed {
        0 => "Pause",
        1 => "x1",
        2 => "x2",
        3 => "x4",
        _ => "Max",
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    None,
    Swarm,
    War,
    Peace,
    /// Bâtir un bâtiment (S8) sur la case cliquée.
    Build(Building),
    /// Démolir le bâtiment de la case cliquée (remboursement partiel).
    Demolish,
    /// Recruter une unité (S5) sur la caserne cliquée.
    Create(UnitKind),
    /// Contrôle des unités : 1er clic = sélection ; 2e clic = déplacer / attaquer.
    Units,
}

/// Catégorie de l'interface de jeu : un **sous-menu** qui regroupe ses outils
/// (épure la barre d'action — on n'affiche que les options de la catégorie ouverte).
#[derive(PartialEq, Clone, Copy)]
enum Category {
    Economy,
    Military,
    Diplomacy,
    Tech,
}

/// Catégorie d'un outil (pour synchroniser le sous-menu ouvert avec l'outil actif).
fn category_of(t: Tool) -> Option<Category> {
    match t {
        Tool::Swarm | Tool::Demolish => Some(Category::Economy),
        Tool::Create(_) | Tool::Units => Some(Category::Military),
        Tool::War | Tool::Peace => Some(Category::Diplomacy),
        Tool::Build(Building::Military) | Tool::Build(Building::Port) => Some(Category::Military),
        Tool::Build(_) => Some(Category::Economy),
        Tool::None => None,
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Menu,
    Settings,
    Game,
}

/// Boutons du menu principal.
#[derive(Clone, Copy, PartialEq)]
enum MenuBtn {
    Play,
    Spectate,
    Settings,
    Quit,
}

/// Boutons de l'écran paramètres.
#[derive(Clone, Copy, PartialEq)]
enum SetBtn {
    SeedDn,
    SeedUp,
    NationsDn,
    NationsUp,
    ZoomDn,
    ZoomUp,
    Fullscreen,
    Back,
}

/// Boutons de l'écran de jeu.
#[derive(Clone, Copy, PartialEq)]
enum GameBtn {
    Menu,
    Tool(Tool),
    /// Ouvre un sous-menu (catégorie).
    Cat(Category),
    Research(u8),
    Speed(u32),
}

/// Entrées d'une frame — abstraites pour piloter l'app aussi bien en réel
/// (fenêtre) qu'en audit (séquence scriptée, sans fenêtre).
#[derive(Default, Clone)]
struct Input {
    pressed: Vec<Key>,
    down: Vec<Key>,
    mx: i32,
    my: i32,
    click: bool,
    scroll: f32,
}

impl Input {
    fn click_at(mx: i32, my: i32) -> Self {
        Input {
            mx,
            my,
            click: true,
            ..Default::default()
        }
    }
    fn key_pressed(&self, k: Key) -> bool {
        self.pressed.contains(&k)
    }
    fn key_down(&self, k: Key) -> bool {
        self.down.contains(&k)
    }
}

/// Touches surveillées chaque frame (pour bâtir l'Input depuis la fenêtre).
const WATCH: [Key; 29] = [
    Key::A,
    Key::D,
    Key::W,
    Key::S,
    Key::Left,
    Key::Right,
    Key::Up,
    Key::Down,
    Key::Escape,
    Key::F,
    Key::E,
    Key::N,
    Key::M,
    Key::B,
    Key::G,
    Key::P,
    Key::I,
    Key::C,
    Key::R,
    Key::U,
    Key::K,
    Key::Y,
    Key::V,
    Key::Space,
    Key::Key0,
    Key::Key1,
    Key::Key2,
    Key::Key3,
    Key::Key4,
];

fn gather_input(window: &Window, mx: i32, my: i32, click: bool, scroll: f32) -> Input {
    let mut inp = Input {
        mx,
        my,
        click,
        scroll,
        ..Default::default()
    };
    for k in WATCH {
        if window.is_key_pressed(k, KeyRepeat::No) {
            inp.pressed.push(k);
        }
        if window.is_key_down(k) {
            inp.down.push(k);
        }
    }
    inp
}

/// Réglages modifiables (menu / paramètres).
struct Config {
    seed: u64,
    nations: u16,
    px: u32,
    pre_turns: usize,
    fullscreen: bool,
    win_w: usize,
    win_h: usize,
}

/// État global de l'application.
struct App {
    config: Config,
    screen: Screen,
    quit: bool,
    recreate: bool,
    buf: Vec<u32>,

    // monde de fond du menu (généré une fois) + cache de son rendu.
    menu_world: Option<World>,
    menu_bg: Vec<u32>,
    menu_bg_wh: (usize, usize),

    // partie en cours.
    world: Option<World>,
    player: u16,
    spectator: bool,
    cam_x: u32,
    cam_y: u32,
    px: u32,
    /// Reliquat sous-tuile du glissé caméra (clic droit) — pixels accumulés.
    pan_acc_x: i32,
    pan_acc_y: i32,
    tool: Tool,
    cat: Category,
    selected: Option<(u32, u32)>,
    /// Unité sélectionnée (id) pour le contrôle (déplacer / attaquer).
    selected_unit: Option<u32>,
    pending_src: Option<(u32, u32)>,
    speed: u32,
    /// Dernière vitesse non nulle (pour reprendre la lecture après une pause).
    last_speed: u32,
    last_instant: Option<Instant>,
    acc_us: i64,
    last_msg: String,
    stats: String,
    stats_dirty: bool,

    // enregistrement (audit total) / rejeu déterministe
    record_path: Option<String>,
    recorder: Option<persist::Recorder>,
    replay_mode: bool,
    replay_cmds: Vec<Command>,
    replay_pos: usize,

    // Directeur temps réel (version intention)
    director: ai::Director,
    director_worker: Option<llm::DirectorWorker>,
    enable_llm: bool,
    last_llm_request: Option<Instant>,
    debug_director: bool,
    director_status: String,
}

fn main() {
    let args = Args::parse();
    if args.audit {
        run_audit(&args);
        return;
    }
    if args.headless {
        run_headless(&args);
        return;
    }
    let mut app = App::new(&args);
    app.enable_llm = true; // jeu fenêtré live : le Directeur LLM s'active si une clé existe
    if let Some(p) = args.replay.clone() {
        if !app.load_replay(&p) {
            return;
        }
    }
    let mut mouse_was_down = false;
    // Pan caméra au clic droit maintenu (glisser-déplacer).
    let mut right_was_down = false;
    let (mut prev_mx, mut prev_my) = (0i32, 0i32);

    loop {
        let fs = app.config.fullscreen;
        // Plein écran : zone de travail (écran moins la barre des tâches) pour
        // que rien ne soit caché ; sinon fenêtre ~90 %.
        let (ox, oy, iw, ih) = if fs {
            work_area()
        } else {
            (0, 0, app.config.win_w as i32, app.config.win_h as i32)
        };
        let opts = WindowOptions {
            resize: !fs,
            borderless: fs,
            title: !fs,
            ..WindowOptions::default()
        };
        let mut window =
            Window::new("ENYO", iw as usize, ih as usize, opts).expect("ouverture de la fenêtre");
        if fs {
            window.set_position(ox as isize, oy as isize);
        }
        window.set_target_fps(60);
        app.recreate = false;

        while window.is_open() && !app.quit && !app.recreate {
            let (w, h) = window.get_size();
            let (mx, my) = window
                .get_mouse_pos(MouseMode::Clamp)
                .map(|(a, b)| (a as i32, b as i32))
                .unwrap_or((-1, -1));
            let down = window.get_mouse_down(MouseButton::Left);
            let click = down && !mouse_was_down;
            mouse_was_down = down;
            let scroll = window.get_scroll_wheel().map(|(_, y)| y).unwrap_or(0.0);
            let (wi, hi) = (w as i32, h as i32);

            // Clic droit maintenu = déplacer librement la caméra (glisser).
            let right_down = window.get_mouse_down(MouseButton::Right);
            if right_down && !right_was_down {
                app.pan_reset(); // début du glissé : repart d'un offset propre
            } else if right_down && right_was_down {
                app.pan_drag(mx - prev_mx, my - prev_my);
            }
            right_was_down = right_down;
            prev_mx = mx;
            prev_my = my;

            let input = gather_input(&window, mx, my, click, scroll);
            app.handle(&input, wi, hi);
            app.draw(wi, hi, mx, my);

            window.update_with_buffer(&app.buf, w, h).expect("affichage");
        }
        if app.quit || !app.recreate {
            break;
        }
    }
}

/// Rend un seul écran en PNG, sans fenêtre (vérification visuelle headless).
/// Avec `--replay`, rejoue tout l'enregistrement et imprime le checksum final
/// (sert à vérifier que le rejeu reproduit la partie au bit près).
fn run_headless(args: &Args) {
    let mut app = App::new(args);
    let (w, h) = (1280usize, 800usize);
    app.buf = vec![gui::BG; w * h];
    let (wi, hi) = (w as i32, h as i32);

    if let Some(p) = args.replay.clone() {
        if !app.load_replay(&p) {
            return;
        }
        app.replay_all();
        // Centre la caméra sur le territoire du joueur et zoome (aperçu utile des
        // bâtiments/saisons), sinon la capture tombe souvent sur l'océan.
        let bbox = app.world.as_ref().and_then(|world| {
            println!("rejeu: tour {} checksum {:016x}", world.turn, world.checksum());
            render::nation_bbox(world, app.player, 3)
        });
        if let Some((x0, y0, bw, bh)) = bbox {
            app.cam_x = x0 + bw / 2;
            app.cam_y = y0 + bh / 2;
        }
        app.px = app.px.max(20);
        app.draw_game(wi, hi, -1, -1);
    } else {
        match args.screen.as_str() {
            "menu" => {
                app.screen = Screen::Menu;
                app.draw_menu(wi, hi, -1, -1);
            }
            "settings" => {
                app.screen = Screen::Settings;
                app.draw_settings(wi, hi, -1, -1);
            }
            _ => {
                app.start_game(args.spectator);
                if let Some(world) = app.world.as_ref() {
                    println!(
                        "partie: tour {} checksum {:016x}",
                        world.turn,
                        world.checksum()
                    );
                }
                app.selected = Some((app.cam_x, app.cam_y)); // aperçu du panneau
                let (hx, hy) = args.hover.unwrap_or((-1, -1));
                app.draw_game(wi, hi, hx, hy);
            }
        }
    }
    let path = args.shot.as_deref().unwrap_or("out/ui.png");
    if let Some(p) = std::path::Path::new(path).parent() {
        if !p.as_os_str().is_empty() {
            std::fs::create_dir_all(p).ok();
        }
    }
    match render::save_argb(&app.buf, w as u32, h as u32, path) {
        Ok(()) => println!("capture écrite: {path}"),
        Err(e) => eprintln!("échec capture: {e}"),
    }
}

/// Petit pilote d'audit : applique des entrées et sauve un PNG par étape.
struct Auditor {
    dir: String,
    w: i32,
    h: i32,
    n: usize,
    shots: Vec<String>,
}

impl Auditor {
    fn snap(&mut self, app: &mut App, label: &str) {
        app.draw(self.w, self.h, -1, -1);
        let path = format!("{}/{:02}_{}.png", self.dir, self.n, label);
        match render::save_argb(&app.buf, self.w as u32, self.h as u32, &path) {
            Ok(()) => self.shots.push(path),
            Err(e) => eprintln!("échec capture {path}: {e}"),
        }
        self.n += 1;
    }
    fn step(&mut self, app: &mut App, input: &Input, label: &str) {
        app.handle(input, self.w, self.h);
        self.snap(app, label);
    }
}

/// Centre d'un bouton identifié dans une liste (sinon (-1,-1)).
fn center_of<T: PartialEq>(list: &[(T, Button)], want: T) -> (i32, i32) {
    list.iter()
        .find(|(id, _)| *id == want)
        .map(|(_, b)| (b.x + b.w / 2, b.y + b.h / 2))
        .unwrap_or((-1, -1))
}

/// Audit « en vrai » : pilote la véritable app (mêmes `handle`/`draw` que la
/// fenêtre) via une séquence d'entrées scriptées et sauve un PNG par étape.
/// Vérifie l'interface ET le jeu en conditions réelles, sans fenêtre bloquante.
fn run_audit(args: &Args) {
    let dir = args.out.clone().unwrap_or_else(|| "out/audit".to_string());
    std::fs::create_dir_all(&dir).ok();
    let (w, h) = if args.fullscreen {
        let wa = work_area();
        (wa.2.max(640), wa.3.max(480))
    } else {
        (1280, 800)
    };
    let mut app = App::new(args);
    app.buf = vec![gui::BG; (w * h) as usize];
    // L'enregistrement de l'audit va dans son propre dossier (ne pas écraser
    // la dernière vraie partie) ; il sert aussi d'artefact rejouable.
    app.record_path = Some(format!("{dir}/audit.rec.jsonl"));
    let mut a = Auditor {
        dir,
        w,
        h,
        n: 0,
        shots: Vec::new(),
    };
    let map_pt = (w / 2, (TOP_H + (h - BOT_H)) / 2);

    // --- Menu -> Paramètres -> retour ---
    app.screen = Screen::Menu;
    a.snap(&mut app, "menu");
    let p = center_of(&app.menu_buttons(w, h), MenuBtn::Settings);
    a.step(&mut app, &Input::click_at(p.0, p.1), "param_ouvert");
    let p = center_of(&app.settings_buttons(w, h), SetBtn::NationsUp);
    a.step(&mut app, &Input::click_at(p.0, p.1), "param_nations_plus");
    let p = center_of(&app.settings_buttons(w, h), SetBtn::Back);
    a.step(&mut app, &Input::click_at(p.0, p.1), "retour_menu");

    // --- Jouer (nation 0) ---
    let p = center_of(&app.menu_buttons(w, h), MenuBtn::Play);
    a.step(&mut app, &Input::click_at(p.0, p.1), "jeu_debut");

    // Quelques fins de tour (le monde évolue). Espace bascule désormais la pause :
    // l'audit avance donc explicitement via `advance()`.
    for i in 0..3 {
        app.advance();
        a.snap(&mut app, &format!("jeu_tour{}", i + 1));
    }

    // --- Sous-menu Économie : bâtir une industrie + laisser produire. ---
    let p = center_of(&app.game_buttons(w, h), GameBtn::Cat(Category::Economy));
    a.step(&mut app, &Input::click_at(p.0, p.1), "menu_economie");
    let p = center_of(
        &app.game_buttons(w, h),
        GameBtn::Tool(Tool::Build(Building::Industry)),
    );
    a.step(&mut app, &Input::click_at(p.0, p.1), "outil_industrie");
    a.step(&mut app, &Input::click_at(map_pt.0, map_pt.1), "industrie_batie");
    app.advance();
    a.snap(&mut app, "industrie_apres_tour");

    // --- Sous-menu Technologie : recherche Essor (succès OU rejet « savoir »). ---
    let p = center_of(&app.game_buttons(w, h), GameBtn::Cat(Category::Tech));
    a.step(&mut app, &Input::click_at(p.0, p.1), "menu_technologie");
    let p = center_of(&app.game_buttons(w, h), GameBtn::Research(0));
    a.step(&mut app, &Input::click_at(p.0, p.1), "recherche_essor");

    // --- Sous-menu Militaire : outil de contrôle des unités. ---
    let p = center_of(&app.game_buttons(w, h), GameBtn::Cat(Category::Military));
    a.step(&mut app, &Input::click_at(p.0, p.1), "menu_militaire");
    let p = center_of(&app.game_buttons(w, h), GameBtn::Tool(Tool::Units));
    a.step(&mut app, &Input::click_at(p.0, p.1), "outil_unites");
    a.step(&mut app, &Input::click_at(map_pt.0, map_pt.1), "unites_clic");

    // Inspecter une case (panneau).
    let p = center_of(&app.game_buttons(w, h), GameBtn::Tool(Tool::None));
    a.step(&mut app, &Input::click_at(p.0, p.1), "outil_inspecter");
    a.step(&mut app, &Input::click_at(map_pt.0, map_pt.1), "inspecter_case");

    // --- Spectateur : évolution du monde sur plusieurs tours ---
    app.start_game(true);
    a.snap(&mut app, "spectateur_t0");
    for stop in [15usize, 30, 60] {
        while app.world.as_ref().map(|w| w.turn).unwrap_or(0) < stop as u64 {
            app.end_turn();
        }
        a.snap(&mut app, &format!("spectateur_t{stop}"));
    }

    println!("audit : {} captures dans {}/", a.shots.len(), a.dir);
    for s in &a.shots {
        println!("  {s}");
    }
}

impl App {
    fn new(args: &Args) -> Self {
        let (sw, sh) = screen_size();
        App {
            config: Config {
                seed: args.seed,
                nations: args.nations,
                px: args.px,
                pre_turns: args.pre_turns,
                fullscreen: args.fullscreen,
                win_w: (sw * 9 / 10).max(960),
                win_h: (sh * 9 / 10).max(600),
            },
            screen: Screen::Menu,
            quit: false,
            recreate: false,
            buf: Vec::new(),
            menu_world: None,
            menu_bg: Vec::new(),
            menu_bg_wh: (0, 0),
            world: None,
            player: 0,
            spectator: false,
            cam_x: 400,
            cam_y: 250,
            pan_acc_x: 0,
            pan_acc_y: 0,
            px: args.px,
            tool: Tool::None,
            cat: Category::Economy,
            selected: None,
            selected_unit: None,
            pending_src: None,
            speed: 0,
            last_speed: 1,
            last_instant: None,
            acc_us: 0,
            last_msg: String::new(),
            stats: String::new(),
            stats_dirty: true,
            record_path: args.record.clone(),
            recorder: None,
            replay_mode: false,
            replay_cmds: Vec::new(),
            replay_pos: 0,
            director: ai::Director::new(),
            director_worker: None,
            enable_llm: false,
            last_llm_request: None,
            debug_director: args.debug_director,
            director_status: "baseline".to_string(),
        }
    }

    // ---- Cycle de partie -------------------------------------------------

    /// Point UNIQUE d'application : enregistre la commande (si actif) puis l'applique.
    /// Tout passe par ici → le `.rec.jsonl` contient le flux complet (joueur,
    /// Step, Directeur, IA) et rejoue la partie à l'identique.
    fn apply(&mut self, cmd: Command) -> Vec<Event> {
        let mut drop_rec = false;
        if let Some(rec) = self.recorder.as_mut() {
            if rec.record(&cmd).is_err() {
                drop_rec = true;
            }
        }
        if drop_rec {
            eprintln!("enregistrement interrompu (erreur d'ecriture)");
            self.recorder = None;
        }
        match self.world.as_mut() {
            Some(w) => w.apply(cmd),
            None => Vec::new(),
        }
    }

    fn start_game(&mut self, spectator: bool) {
        let seed = self.config.seed;
        self.world = Some(World::new(seed, 800, 500));
        self.spectator = spectator;
        self.replay_mode = false;
        // Directeur : repart neuf ; worker LLM seulement en jeu live (avec clé).
        self.director = ai::Director::new();
        self.director_worker = if self.enable_llm {
            llm::DeepSeek::from_env().map(llm::DirectorWorker::spawn)
        } else {
            None
        };
        self.last_llm_request = None;
        self.director_status = if self.director_worker.is_some() {
            "LLM actif".to_string()
        } else {
            "baseline".to_string()
        };
        // Enregistrement auto (audit total) : la dernière partie est rejouable.
        self.recorder = None;
        let path = self
            .record_path
            .clone()
            .unwrap_or_else(|| "out/derniere-partie.rec.jsonl".to_string());
        match persist::Recorder::create(
            &path,
            &persist::Header {
                seed,
                width: 800,
                height: 500,
            },
        ) {
            Ok(r) => self.recorder = Some(r),
            Err(e) => eprintln!("enregistrement impossible ({path}): {e}"),
        }
        let setup = ai::spawn_nations(self.world.as_ref().unwrap(), self.config.nations, self.player);
        for c in setup {
            self.apply(c);
        }
        for _ in 0..self.config.pre_turns {
            self.end_turn();
        }
        let world = self.world.as_ref().unwrap();
        let (cx, cy) = render::nation_bbox(world, self.player, 0)
            .map(|(x, y, w, h)| (x + w / 2, y + h / 2))
            .unwrap_or((world.width / 2, world.height / 2));
        self.cam_x = cx;
        self.cam_y = cy;
        self.px = self.config.px;
        self.tool = Tool::None;
        self.cat = Category::Economy;
        self.selected = None;
        self.selected_unit = None;
        self.pending_src = None;
        self.speed = if spectator { 1 } else { 0 }; // Jouer = départ en pause
        self.last_instant = None;
        self.acc_us = 0;
        self.last_msg.clear();
        self.stats_dirty = true;
        self.screen = Screen::Game;
    }

    fn end_turn(&mut self) {
        if self.world.is_none() {
            return;
        }
        let (player, nations, spec) = (self.player, self.config.nations, self.spectator);
        self.apply(Command::Step);
        // Directeur : résout l'intention COURANTE contre l'état COURANT.
        let dir_cmds = self
            .director
            .resolve_tick(self.world.as_ref().unwrap(), player);
        for c in dir_cmds {
            self.apply(c);
        }
        for nid in 0..nations {
            if !spec && nid == player {
                continue;
            }
            for c in ai::plan(self.world.as_ref().unwrap(), nid) {
                self.apply(c);
            }
        }
        self.stats_dirty = true;
    }

    /// Charge un enregistrement et bascule en mode rejeu (état = mise en place).
    fn load_replay(&mut self, path: &str) -> bool {
        let (header, cmds) = match persist::read_recording(path) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("rejeu impossible ({path}): {e}");
                return false;
            }
        };
        self.replay_mode = true;
        self.recorder = None;
        self.director_worker = None; // le rejeu ne consulte jamais le LLM
        self.config.seed = header.seed;
        let mut world = World::new(header.seed, header.width, header.height);
        // Applique la mise en place : tout ce qui précède le 1er Step (tour 0).
        let mut pos = 0;
        while pos < cmds.len() && !matches!(cmds[pos], Command::Step) {
            world.apply(cmds[pos].clone());
            pos += 1;
        }
        let (cx, cy) = render::nation_bbox(&world, self.player, 0)
            .map(|(x, y, w, h)| (x + w / 2, y + h / 2))
            .unwrap_or((world.width / 2, world.height / 2));
        self.cam_x = cx;
        self.cam_y = cy;
        self.px = self.config.px;
        self.world = Some(world);
        self.replay_cmds = cmds;
        self.replay_pos = pos;
        self.spectator = true; // pas d'action joueur en rejeu (Pause/x1/x2 dispo)
        self.speed = 0;
        self.last_instant = None;
        self.acc_us = 0;
        self.screen = Screen::Game;
        self.last_msg = "rejeu pret - Espace ou x1/x2 pour derouler".to_string();
        self.stats_dirty = true;
        true
    }

    /// Avance le rejeu d'UN tour : applique le Step courant + tout jusqu'au prochain.
    fn replay_step(&mut self) {
        if self.replay_pos >= self.replay_cmds.len() {
            self.last_msg = "fin du rejeu".to_string();
            return;
        }
        let mut first = true;
        while self.replay_pos < self.replay_cmds.len() {
            let is_step = matches!(self.replay_cmds[self.replay_pos], Command::Step);
            if is_step && !first {
                break;
            }
            let cmd = self.replay_cmds[self.replay_pos].clone();
            if let Some(w) = self.world.as_mut() {
                w.apply(cmd);
            }
            self.replay_pos += 1;
            first = false;
        }
        self.stats_dirty = true;
    }

    /// Applique tout le reste du rejeu d'un coup (état final).
    fn replay_all(&mut self) {
        while self.replay_pos < self.replay_cmds.len() {
            let cmd = self.replay_cmds[self.replay_pos].clone();
            if let Some(w) = self.world.as_mut() {
                w.apply(cmd);
            }
            self.replay_pos += 1;
        }
        self.stats_dirty = true;
    }

    fn ensure_menu_world(&mut self) {
        if self.menu_world.is_none() {
            let mut world = World::new(self.config.seed, 800, 500);
            for c in ai::spawn_nations(&world, self.config.nations, self.player) {
                world.apply(c);
            }
            for _ in 0..40 {
                step_world(&mut world, 0, self.config.nations, true);
            }
            self.menu_world = Some(world);
        }
    }

    fn recompute_stats(&mut self) {
        let Some(world) = self.world.as_ref() else {
            return;
        };
        let (pop, tiles) = world.nation_stats(self.player);
        let prov = world
            .provinces()
            .iter()
            .filter(|p| p.owner == self.player)
            .count();
        let year = world.turn / 12;
        let month = world.turn % 12 + 1;
        // Guerres en cours : score d'occupation / seuil de capitulation (> 75 %).
        let mut war = String::new();
        for &(a, b) in world.diplomacy.wars() {
            let enemy = if a == self.player {
                Some(b)
            } else if b == self.player {
                Some(a)
            } else {
                None
            };
            if let Some(e) = enemy {
                let score = world.war_score(self.player, e);
                let need = world.nation_value(e) * 3 / 4 + 1;
                war.push_str(&format!("  GUERRE N{e}: {score}/{need}"));
            }
        }
        // Les ressources sont rendues en « chips » survolables (cf. draw_game).
        self.stats = format!(
            "An {year} M{month:02} N{}  {pop:.0}h {tiles}c {prov}p{war}",
            self.player
        );
    }

    // ---- Entrées ---------------------------------------------------------

    fn handle_menu(&mut self, input: &Input, w: i32, h: i32) {
        if input.key_pressed(Key::Escape) {
            self.quit = true;
        }
        if !input.click {
            return;
        }
        for (id, b) in self.menu_buttons(w, h) {
            if b.hit(input.mx, input.my) {
                match id {
                    MenuBtn::Play => self.start_game(false),
                    MenuBtn::Spectate => self.start_game(true),
                    MenuBtn::Settings => self.screen = Screen::Settings,
                    MenuBtn::Quit => self.quit = true,
                }
                return;
            }
        }
    }

    fn handle_settings(&mut self, input: &Input, w: i32, h: i32) {
        if input.key_pressed(Key::Escape) {
            self.screen = Screen::Menu;
        }
        if !input.click {
            return;
        }
        for (id, b) in self.settings_buttons(w, h) {
            if b.hit(input.mx, input.my) {
                match id {
                    SetBtn::SeedDn => self.config.seed = self.config.seed.wrapping_sub(1),
                    SetBtn::SeedUp => self.config.seed = self.config.seed.wrapping_add(1),
                    SetBtn::NationsDn => self.config.nations = self.config.nations.saturating_sub(1).max(2),
                    SetBtn::NationsUp => self.config.nations = (self.config.nations + 1).min(12),
                    SetBtn::ZoomDn => self.config.px = self.config.px.saturating_sub(2).max(6),
                    SetBtn::ZoomUp => self.config.px = (self.config.px + 2).min(40),
                    SetBtn::Fullscreen => {
                        self.config.fullscreen = !self.config.fullscreen;
                        self.recreate = true;
                    }
                    SetBtn::Back => {
                        // les réglages de monde changent : régénérer le fond du menu.
                        self.menu_world = None;
                        self.menu_bg_wh = (0, 0);
                        self.screen = Screen::Menu;
                    }
                }
                return;
            }
        }
    }

    /// Début d'un cliqué-glissé droit : on repart d'un reliquat propre pour
    /// éviter tout saut hérité d'un glissé précédent.
    fn pan_reset(&mut self) {
        self.pan_acc_x = 0;
        self.pan_acc_y = 0;
    }

    /// Glisser-déplacer la caméra au clic droit maintenu : on « attrape » la
    /// carte, qui suit le curseur (la caméra va donc à l'inverse du mouvement
    /// souris). `dpx`/`dpy` = déplacement souris en pixels depuis la frame
    /// précédente ; on convertit en tuiles via `px` (pixels/tuile) et on garde
    /// le reliquat sous-tuile pour un défilement fluide.
    fn pan_drag(&mut self, dpx: i32, dpy: i32) {
        if self.screen != Screen::Game {
            return;
        }
        let (ww, wh) = self
            .world
            .as_ref()
            .map(|w| (w.width, w.height))
            .unwrap_or((800, 500));
        let step = self.px.max(1) as i32; // pixels par tuile
        self.pan_acc_x += dpx;
        self.pan_acc_y += dpy;
        while self.pan_acc_x >= step {
            self.cam_x = self.cam_x.saturating_sub(1);
            self.pan_acc_x -= step;
        }
        while self.pan_acc_x <= -step {
            self.cam_x = (self.cam_x + 1).min(ww.saturating_sub(1));
            self.pan_acc_x += step;
        }
        while self.pan_acc_y >= step {
            self.cam_y = self.cam_y.saturating_sub(1);
            self.pan_acc_y -= step;
        }
        while self.pan_acc_y <= -step {
            self.cam_y = (self.cam_y + 1).min(wh.saturating_sub(1));
            self.pan_acc_y += step;
        }
    }

    fn handle_game(&mut self, input: &Input, w: i32, h: i32) {
        let (ww, wh) = self
            .world
            .as_ref()
            .map(|w| (w.width, w.height))
            .unwrap_or((800, 500));

        // Déplacement (touche maintenue).
        let pan = 3;
        if input.key_down(Key::A) || input.key_down(Key::Left) {
            self.cam_x = self.cam_x.saturating_sub(pan);
        }
        if input.key_down(Key::D) || input.key_down(Key::Right) {
            self.cam_x = (self.cam_x + pan).min(ww - 1);
        }
        if input.key_down(Key::W) || input.key_down(Key::Up) {
            self.cam_y = self.cam_y.saturating_sub(pan);
        }
        if input.key_down(Key::S) || input.key_down(Key::Down) {
            self.cam_y = (self.cam_y + pan).min(wh - 1);
        }
        if input.scroll > 0.0 {
            self.px = (self.px + 2).min(40);
        } else if input.scroll < 0.0 {
            self.px = self.px.saturating_sub(2).max(6);
        }

        // Échap : retour menu.
        if input.key_pressed(Key::Escape) {
            self.screen = Screen::Menu;
            return;
        }
        // Outils + recherche : uniquement quand on JOUE (pas en rejeu).
        // (A/W/S/D sont le déplacement → on évite ces lettres pour les outils.)
        if !self.replay_mode {
            for (k, t) in [
                (Key::E, Tool::Swarm),
                (Key::N, Tool::None),
                (Key::M, Tool::Units),
                (Key::G, Tool::War),
                (Key::P, Tool::Peace),
                (Key::I, Tool::Build(Building::Industry)),
                (Key::C, Tool::Build(Building::Commerce)),
                (Key::R, Tool::Build(Building::Infrastructure)),
                (Key::U, Tool::Build(Building::Education)),
                (Key::K, Tool::Build(Building::Military)),
                (Key::Y, Tool::Build(Building::Farm)),
                (Key::V, Tool::Build(Building::City)),
            ] {
                if input.key_pressed(k) {
                    self.set_tool(t);
                }
            }
            if !self.spectator {
                for (k, b) in [
                    (Key::Key1, 0u8),
                    (Key::Key2, 1),
                    (Key::Key3, 2),
                    (Key::Key4, 3),
                ] {
                    if input.key_pressed(k) {
                        self.research(b);
                    }
                }
            }
        }
        // Espace = lecture / pause (bascule), jeu comme rejeu.
        if input.key_pressed(Key::Space) {
            self.toggle_pause();
        }
        // Touches vitesse : disponibles quand les chiffres ne servent pas a la
        // recherche, c.-a-d. en spectateur et en rejeu (spectator==true).
        if self.spectator {
            for (k, s) in [
                (Key::Key0, 0u32),
                (Key::Key1, 1),
                (Key::Key2, 2),
                (Key::Key3, 3),
                (Key::Key4, 4),
            ] {
                if input.key_pressed(k) {
                    self.speed = s;
                    if s != 0 {
                        self.last_speed = s;
                    }
                }
            }
        }

        // Horloge murale : avance le monde (jeu, spectateur, rejeu) selon la
        // vitesse. Confinee a `ui` : ne decide QUE combien de Step et quand.
        let now = Instant::now();
        let dt_us = self
            .last_instant
            .map(|l| now.duration_since(l).as_micros() as i64)
            .unwrap_or(0);
        self.last_instant = Some(now); // remis a chaque frame, meme en pause
        match self.speed {
            0 => self.acc_us = 0, // pause : on n'accumule pas
            s if s >= 4 => {
                // Max : rafale bornee (fast-forward / observation rapide)
                for _ in 0..MAX_TICKS_PER_FRAME {
                    self.advance();
                }
            }
            s => {
                let period = tick_period_us(s);
                self.acc_us += dt_us;
                let mut n = 0;
                while self.acc_us >= period && n < MAX_TICKS_PER_FRAME {
                    self.advance();
                    self.acc_us -= period;
                    n += 1;
                }
                if self.acc_us > period {
                    self.acc_us = 0; // garde anti-spirale : on jette le surplus
                }
            }
        }

        // Directeur LLM (asynchrone) : récolte un résultat, relance si dû.
        if self.director_worker.is_some() {
            let mut new_intent = None;
            let mut status = None;
            let mut fired = false;
            if let Some(worker) = self.director_worker.as_ref() {
                match worker.poll() {
                    Some(Ok(intent)) => {
                        status = Some(format!(
                            "LLM {:?} i{} <<{}>>",
                            intent.stance, intent.intensity, intent.public_cause
                        ));
                        new_intent = Some(intent);
                    }
                    Some(Err(e)) => status = Some(format!("LLM echec: {e}")),
                    None => {}
                }
                let due = self
                    .last_llm_request
                    .map(|t| now.duration_since(t).as_secs() >= LLM_MIN_SECS)
                    .unwrap_or(true);
                if self.speed > 0 && due && !worker.busy() {
                    if let Some(world) = self.world.as_ref() {
                        let view = llm::DirectorView::from_world(world, self.player);
                        fired = worker.request(view);
                    }
                }
            }
            if let Some(i) = new_intent {
                let turn_now = self.world.as_ref().map(|w| w.turn).unwrap_or(0);
                self.director.set_intent(i, turn_now); // ré-ancre until_turn (M2)
            }
            if let Some(s) = status {
                self.director_status = s;
            }
            if fired {
                self.last_llm_request = Some(now);
            }
        }

        // Clic : boutons d'abord, sinon la carte.
        if input.click {
            for (id, b) in self.game_buttons(w, h) {
                if b.hit(input.mx, input.my) {
                    self.do_game_btn(id);
                    return;
                }
            }
            if input.my > TOP_H && input.my < h - BOT_H {
                self.map_click(input.mx, input.my, w, h);
            }
        }
    }

    /// Dispatch entrée selon l'écran courant.
    fn handle(&mut self, input: &Input, w: i32, h: i32) {
        match self.screen {
            Screen::Menu => self.handle_menu(input, w, h),
            Screen::Settings => self.handle_settings(input, w, h),
            Screen::Game => self.handle_game(input, w, h),
        }
    }

    /// Dispatch rendu selon l'écran courant (après `handle`, donc écran à jour).
    fn draw(&mut self, w: i32, h: i32, mx: i32, my: i32) {
        match self.screen {
            Screen::Menu => self.draw_menu(w, h, mx, my),
            Screen::Settings => self.draw_settings(w, h, mx, my),
            Screen::Game => self.draw_game(w, h, mx, my),
        }
    }

    fn set_tool(&mut self, t: Tool) {
        self.tool = t;
        // Garde le sous-menu ouvert cohérent avec l'outil choisi (au clavier aussi).
        if let Some(c) = category_of(t) {
            self.cat = c;
        }
        self.pending_src = None;
        // Quitter le contrôle d'unités désélectionne l'unité courante.
        if !matches!(t, Tool::Units) {
            self.selected_unit = None;
        }
    }

    fn research(&mut self, branch: u8) {
        let player = self.player;
        let ev = self.apply(Command::Research {
            nation: player,
            branch,
        });
        if let Some(m) = feedback(&ev) {
            self.last_msg = m;
        }
        self.stats_dirty = true;
    }

    /// Tour suivant : déroule le rejeu si actif, sinon résout un vrai tour.
    /// (Utilisé par l'audit scripté ; en jeu, Espace bascule la pause.)
    fn advance(&mut self) {
        if self.replay_mode {
            self.replay_step();
        } else {
            self.end_turn();
        }
    }

    /// Espace : bascule lecture/pause. La lecture reprend la dernière vitesse.
    fn toggle_pause(&mut self) {
        if self.speed == 0 {
            self.speed = self.last_speed.max(1);
        } else {
            self.last_speed = self.speed;
            self.speed = 0;
        }
    }

    fn do_game_btn(&mut self, id: GameBtn) {
        match id {
            GameBtn::Menu => self.screen = Screen::Menu,
            GameBtn::Tool(t) => {
                if !self.replay_mode {
                    self.set_tool(t);
                }
            }
            GameBtn::Cat(c) => self.cat = c,
            GameBtn::Research(b) => {
                if !self.replay_mode {
                    self.research(b);
                }
            }
            GameBtn::Speed(s) => {
                self.speed = s;
                if s != 0 {
                    self.last_speed = s;
                }
            }
        }
    }

    fn map_click(&mut self, mx: i32, my: i32, w: i32, h: i32) {
        let (tx, ty) = {
            let Some(world) = self.world.as_ref() else {
                return;
            };
            let (x0, y0, _, _, pxe) =
                render::viewport_rect(world, self.cam_x, self.cam_y, self.px, w as u32, h as u32);
            let tx = x0 + (mx as u32) / pxe;
            let ty = y0 + (my as u32) / pxe;
            if tx >= world.width || ty >= world.height {
                return;
            }
            (tx, ty)
        };
        self.selected = Some((tx, ty));
        if self.replay_mode {
            return; // en rejeu : inspection seulement
        }
        let player = self.player;
        let owner = self.world.as_ref().and_then(|w| w.tile(tx, ty).owner);
        match self.tool {
            Tool::Swarm => {
                if let Some((sx, sy)) = self.pending_src.take() {
                    let ev = self.apply(Command::Swarm {
                        from_x: sx,
                        from_y: sy,
                        to_x: tx,
                        to_y: ty,
                    });
                    self.report(&ev);
                } else {
                    self.pending_src = Some((tx, ty));
                    self.last_msg = format!("source expansion ({tx},{ty}) - clique la cible");
                }
            }
            Tool::War => match owner {
                Some(o) if o != player => {
                    let ev = self.apply(Command::DeclareWar {
                        nation: player,
                        target: o,
                    });
                    self.report(&ev);
                }
                Some(_) => self.last_msg = "REJET : c'est ta propre case".to_string(),
                None => self.last_msg = "REJET : case libre (aucune nation)".to_string(),
            },
            Tool::Peace => match owner {
                Some(o) if o != player => {
                    let ev = self.apply(Command::MakePeace {
                        nation: player,
                        target: o,
                    });
                    self.report(&ev);
                }
                _ => self.last_msg = "REJET : choisis une case ennemie".to_string(),
            },
            Tool::Build(b) => {
                let ev = self.apply(Command::Build {
                    x: tx,
                    y: ty,
                    nation: player,
                    building: b,
                });
                self.report(&ev);
            }
            Tool::Demolish => {
                let ev = self.apply(Command::Demolish {
                    x: tx,
                    y: ty,
                    nation: player,
                });
                self.report(&ev);
            }
            Tool::Create(kind) => {
                let ev = self.apply(Command::CreateUnit {
                    x: tx,
                    y: ty,
                    nation: player,
                    kind,
                });
                self.report(&ev);
            }
            Tool::Units => {
                // Infos (unité cliquée, unité sélectionnée, terrain) lues d'un coup.
                let info = self.world.as_ref().map(|w| {
                    let hit = w
                        .units
                        .iter()
                        .find(|u| u.x == tx && u.y == ty)
                        .map(|u| (u.id, u.owner, u.kind));
                    let sel = self
                        .selected_unit
                        .and_then(|id| w.units.iter().find(|u| u.id == id).map(|u| (id, u.kind)));
                    let is_land = w.tile(tx, ty).kind == sim::tile::TileKind::Land;
                    (hit, sel, is_land)
                });
                if let Some((hit, sel, is_land)) = info {
                    match (sel, hit) {
                        // Embarquer : unité terrestre sélectionnée, clic sur SA galère.
                        (Some((selid, selk)), Some((gid, go, UnitKind::Galley)))
                            if go == player && !sim::unit::unit_stats(selk).naval =>
                        {
                            let ev = self.apply(Command::Embark {
                                unit: selid,
                                transport: gid,
                            });
                            self.report(&ev);
                        }
                        // Sélectionner une de ses unités.
                        (_, Some((id, o, _))) if o == player => {
                            self.selected_unit = Some(id);
                            self.last_msg = format!("unite {id} selectionnee");
                        }
                        // Attaquer une unité ennemie.
                        (Some((selid, _)), Some(_)) => {
                            let ev = self.apply(Command::AttackUnit {
                                unit: selid,
                                x: tx,
                                y: ty,
                            });
                            self.report(&ev);
                        }
                        // Galère sélectionnée + clic terre côtière vide -> débarquer.
                        (Some((selid, UnitKind::Galley)), None) if is_land => {
                            let ev = self.apply(Command::Disembark {
                                transport: selid,
                                to_x: tx,
                                to_y: ty,
                            });
                            self.report(&ev);
                        }
                        // Sinon : déplacer.
                        (Some((selid, _)), None) => {
                            let ev = self.apply(Command::MoveUnit {
                                unit: selid,
                                to_x: tx,
                                to_y: ty,
                            });
                            self.report(&ev);
                        }
                        (None, _) => {
                            self.last_msg = "clique une de tes unites".to_string();
                        }
                    }
                }
                // L'unité sélectionnée a pu mourir (riposte) -> désélectionner.
                if let Some(sel) = self.selected_unit {
                    let alive = self
                        .world
                        .as_ref()
                        .map(|w| w.units.iter().any(|u| u.id == sel))
                        .unwrap_or(false);
                    if !alive {
                        self.selected_unit = None;
                    }
                }
            }
            Tool::None => {}
        }
    }

    /// Met à jour le message d'action + marque les stats à recalculer.
    fn report(&mut self, ev: &[Event]) {
        if let Some(m) = feedback(ev) {
            self.last_msg = m;
        }
        self.stats_dirty = true;
    }

    // ---- Mises en page des boutons --------------------------------------

    fn menu_buttons(&self, w: i32, _h: i32) -> Vec<(MenuBtn, Button)> {
        let bw = 280;
        let bh = 52;
        let gap = 16;
        let cx = w / 2 - bw / 2;
        let y0 = 300;
        vec![
            (MenuBtn::Play, Button::new(cx, y0, bw, bh, "Jouer")),
            (
                MenuBtn::Spectate,
                Button::new(cx, y0 + (bh + gap), bw, bh, "Spectateur"),
            ),
            (
                MenuBtn::Settings,
                Button::new(cx, y0 + 2 * (bh + gap), bw, bh, "Parametres"),
            ),
            (
                MenuBtn::Quit,
                Button::new(cx, y0 + 3 * (bh + gap), bw, bh, "Quitter"),
            ),
        ]
    }

    fn settings_buttons(&self, w: i32, _h: i32) -> Vec<(SetBtn, Button)> {
        let cx = w / 2;
        let row = |i: i32| 220 + i * 64;
        let s = 36; // côté des petits boutons +/-
        let bx = cx + 60; // colonne des +/-
        vec![
            (SetBtn::SeedDn, Button::new(bx, row(0), s, s, "-")),
            (SetBtn::SeedUp, Button::new(bx + 120, row(0), s, s, "+")),
            (SetBtn::NationsDn, Button::new(bx, row(1), s, s, "-")),
            (SetBtn::NationsUp, Button::new(bx + 120, row(1), s, s, "+")),
            (SetBtn::ZoomDn, Button::new(bx, row(2), s, s, "-")),
            (SetBtn::ZoomUp, Button::new(bx + 120, row(2), s, s, "+")),
            (
                SetBtn::Fullscreen,
                Button::new(bx, row(3), 156, s, if self.config.fullscreen { "Oui" } else { "Non" }),
            ),
            (SetBtn::Back, Button::new(cx - 90, row(5), 180, 48, "Retour")),
        ]
    }

    fn game_buttons(&self, w: i32, h: i32) -> Vec<(GameBtn, Button)> {
        let mut v = Vec::new();
        let pad = 8;
        // Haut-droite : Fin de tour, Menu (largeurs ajustées au texte, échelle 2).
        let bh = 28;
        let menu_w = gui::text_w("Menu", 2) + 20;
        let mxn = w - pad - menu_w;
        v.push((GameBtn::Menu, Button::new(mxn, 6, menu_w, bh, "Menu")));
        // (Plus de bouton « Fin de tour » : Espace bascule la pause ; la vitesse
        // pilote l'écoulement du temps réel.)

        // Bas, 3 rangées : actions ; bâtir ; recherche + vitesse.
        let tbh = 28;
        let row1 = h - BOT_H + 8;
        let row2 = h - BOT_H + 44;
        let row3 = h - BOT_H + 80;
        let playing = !self.spectator && !self.replay_mode;

        // Rangée 1 : Inspecter (toujours) + catégories (sous-menus, mode Jeu).
        let mut x = pad;
        {
            let lbl = "Inspecter";
            let bw = gui::text_w(lbl, 2) + 18;
            v.push((GameBtn::Tool(Tool::None), Button::new(x, row1, bw, tbh, lbl)));
            x += bw + 6;
        }
        if playing {
            for (lbl, c) in [
                ("Economie", Category::Economy),
                ("Militaire", Category::Military),
                ("Diplomatie", Category::Diplomacy),
                ("Technologie", Category::Tech),
            ] {
                let bw = gui::text_w(lbl, 2) + 18;
                v.push((GameBtn::Cat(c), Button::new(x, row1, bw, tbh, lbl)));
                x += bw + 6;
            }
        }

        // Rangée 2 : sous-options de la catégorie ouverte (mode Jeu).
        if playing {
            let mut x = pad;
            let push_tool = |v: &mut Vec<(GameBtn, Button)>, x: &mut i32, lbl: &str, t: Tool| {
                let bw = gui::text_w(lbl, 2) + 18;
                v.push((GameBtn::Tool(t), Button::new(*x, row2, bw, tbh, lbl)));
                *x += bw + 6;
            };
            match self.cat {
                Category::Economy => {
                    push_tool(&mut v, &mut x, "Etendre", Tool::Swarm);
                    push_tool(&mut v, &mut x, "Ville", Tool::Build(Building::City));
                    push_tool(&mut v, &mut x, "Industrie", Tool::Build(Building::Industry));
                    push_tool(&mut v, &mut x, "Commerce", Tool::Build(Building::Commerce));
                    push_tool(&mut v, &mut x, "Infrastructure", Tool::Build(Building::Infrastructure));
                    push_tool(&mut v, &mut x, "Ferme", Tool::Build(Building::Farm));
                    push_tool(&mut v, &mut x, "Education", Tool::Build(Building::Education));
                    push_tool(&mut v, &mut x, "Demolir", Tool::Demolish);
                }
                Category::Military => {
                    push_tool(&mut v, &mut x, "Unites", Tool::Units);
                    push_tool(&mut v, &mut x, "Infanterie", Tool::Create(UnitKind::Infantry));
                    push_tool(&mut v, &mut x, "Archers", Tool::Create(UnitKind::Archer));
                    push_tool(&mut v, &mut x, "Cavalerie", Tool::Create(UnitKind::Cavalry));
                    push_tool(&mut v, &mut x, "Galere", Tool::Create(UnitKind::Galley));
                    push_tool(&mut v, &mut x, "Caserne", Tool::Build(Building::Military));
                    push_tool(&mut v, &mut x, "Port", Tool::Build(Building::Port));
                }
                Category::Diplomacy => {
                    push_tool(&mut v, &mut x, "Guerre", Tool::War);
                    push_tool(&mut v, &mut x, "Paix", Tool::Peace);
                }
                Category::Tech => {
                    for (i, lbl) in ["Essor", "Terroir", "Fer", "Lien"].iter().enumerate() {
                        let bw = gui::text_w(lbl, 2) + 18;
                        v.push((GameBtn::Research(i as u8), Button::new(x, row2, bw, tbh, *lbl)));
                        x += bw + 6;
                    }
                }
            }
        }

        // Rangée 3 : vitesse (tous modes).
        let mut x = pad;
        for (lbl, s) in [("Pause", 0u32), ("x1", 1), ("x2", 2), ("x4", 3), ("Max", 4)] {
            let bw = gui::text_w(lbl, 2) + 16;
            v.push((GameBtn::Speed(s), Button::new(x, row3, bw, tbh, lbl)));
            x += bw + 6;
        }
        v
    }

    // ---- Rendu -----------------------------------------------------------

    fn draw_menu(&mut self, w: i32, h: i32, mx: i32, my: i32) {
        let (uw, uh) = (w as usize, h as usize);
        // Fond : aperçu du monde, assombri (mis en cache par taille).
        self.ensure_menu_world();
        if self.menu_bg_wh != (uw, uh) {
            if let Some(world) = self.menu_world.as_ref() {
                let mut bg = render::viewport_argb(
                    world,
                    world.width / 2,
                    world.height / 2,
                    3,
                    w as u32,
                    h as u32,
                );
                for p in bg.iter_mut() {
                    *p = scale_rgb(*p, 42); // ~16 % de luminosité
                }
                self.menu_bg = bg;
                self.menu_bg_wh = (uw, uh);
            }
        }

        let mut buf = std::mem::take(&mut self.buf);
        if buf.len() == self.menu_bg.len() && !self.menu_bg.is_empty() {
            buf.copy_from_slice(&self.menu_bg);
        } else {
            buf = vec![gui::BG; uw * uh];
        }
        let buttons = self.menu_buttons(w, h);
        {
            let mut c = Canvas::new(&mut buf, uw, uh);
            // Titre.
            c.text_centered(w / 2, 150, "ENYO", 12, gui::TEXT);
            c.text_centered(
                w / 2,
                262,
                "strategie minimaliste a l'echelle du monde",
                2,
                gui::TEXT_DIM,
            );
            for (id, b) in &buttons {
                let hover = b.hit(mx, my);
                let _ = id;
                b.draw(&mut c, hover, false);
            }
            c.text_centered(
                w / 2,
                h - 40,
                "Echap = quitter   |   souris = naviguer",
                1,
                gui::TEXT_DIM,
            );
        }
        self.buf = buf;
    }

    fn draw_settings(&mut self, w: i32, h: i32, mx: i32, my: i32) {
        let (uw, uh) = (w as usize, h as usize);
        let mut buf = std::mem::take(&mut self.buf);
        if buf.len() != uw * uh {
            buf = vec![gui::BG; uw * uh];
        } else {
            for p in buf.iter_mut() {
                *p = gui::BG;
            }
        }
        let cx = w / 2;
        let row = |i: i32| 220 + i * 64;
        let labels = [
            ("Graine (seed)", format!("{}", self.config.seed)),
            ("Nations", format!("{}", self.config.nations)),
            ("Zoom initial", format!("{}", self.config.px)),
            ("Plein ecran", String::new()),
        ];
        let buttons = self.settings_buttons(w, h);
        {
            let mut c = Canvas::new(&mut buf, uw, uh);
            c.text_centered(cx, 120, "PARAMETRES", 6, gui::TEXT);
            for (i, (name, val)) in labels.iter().enumerate() {
                let y = row(i as i32) + 8;
                c.text(cx - 360, y, name, 2, gui::TEXT);
                if !val.is_empty() {
                    c.text_centered(cx + 78 + 60, y, val, 2, gui::ACCENT_HI);
                }
            }
            for (id, b) in &buttons {
                let hover = b.hit(mx, my);
                let active = matches!(id, SetBtn::Fullscreen) && self.config.fullscreen;
                b.draw(&mut c, hover, active);
            }
            c.text_centered(cx, h - 40, "Echap = retour", 1, gui::TEXT_DIM);
        }
        self.buf = buf;
    }

    fn draw_game(&mut self, w: i32, h: i32, mx: i32, my: i32) {
        if self.stats_dirty {
            self.recompute_stats();
            self.stats_dirty = false;
        }
        let (uw, uh) = (w as usize, h as usize);
        let mut buf = std::mem::take(&mut self.buf);
        if buf.len() != uw * uh {
            buf = vec![gui::BG; uw * uh];
        }
        // Carte plein cadre.
        let mut rect = (0u32, 0u32, 0u32, 0u32, 1u32);
        if let Some(world) = self.world.as_ref() {
            let map =
                render::viewport_argb(world, self.cam_x, self.cam_y, self.px, w as u32, h as u32);
            let n = buf.len().min(map.len());
            buf[..n].copy_from_slice(&map[..n]);
            rect = render::viewport_rect(world, self.cam_x, self.cam_y, self.px, w as u32, h as u32);
        }
        let buttons = self.game_buttons(w, h);
        let toolname = match self.tool {
            Tool::None => "Inspecter",
            Tool::Swarm => "Étendre",
            Tool::War => "Guerre",
            Tool::Peace => "Paix",
            Tool::Build(Building::City) => "Ville",
            Tool::Build(Building::Industry) => "Industrie",
            Tool::Build(Building::Commerce) => "Commerce",
            Tool::Build(Building::Infrastructure) => "Infrastructure",
            Tool::Build(Building::Education) => "Education",
            Tool::Build(Building::Military) => "Caserne",
            Tool::Build(Building::Farm) => "Ferme",
            Tool::Build(Building::Port) => "Port",
            Tool::Demolish => "Demolir",
            Tool::Create(UnitKind::Infantry) => "Infanterie",
            Tool::Create(UnitKind::Archer) => "Archers",
            Tool::Create(UnitKind::Cavalry) => "Cavalerie",
            Tool::Create(UnitKind::Galley) => "Galere",
            Tool::Units => "Unites",
        };
        let tech = self
            .world
            .as_ref()
            .and_then(|w| w.nation(self.player))
            .map(|n| n.tech)
            .unwrap_or_default();
        {
            let mut c = Canvas::new(&mut buf, uw, uh);
            // Surbrillance de la case sélectionnée / source d'expansion.
            let (x0, y0, cols, rows, pxe) = rect;
            let mark = |c: &mut Canvas, t: Option<(u32, u32)>, col: u32| {
                if let Some((tx, ty)) = t {
                    if tx >= x0 && ty >= y0 && tx < x0 + cols && ty < y0 + rows {
                        let sx = ((tx - x0) * pxe) as i32;
                        let sy = ((ty - y0) * pxe) as i32;
                        c.rect_outline(sx, sy, pxe as i32, pxe as i32, col);
                        c.rect_outline(sx - 1, sy - 1, pxe as i32 + 2, pxe as i32 + 2, col);
                    }
                }
            };
            mark(&mut c, self.pending_src, gui::GOOD);
            mark(&mut c, self.selected, gui::TEXT);
            // Surbrillance de l'unité sélectionnée (cyan).
            let sel_unit_pos = self.selected_unit.and_then(|id| {
                self.world
                    .as_ref()
                    .and_then(|w| w.units.iter().find(|u| u.id == id).map(|u| (u.x, u.y)))
            });
            mark(&mut c, sel_unit_pos, 0x66_E0FF);

            // Infobulle éventuelle (dessinée en dernier, au-dessus de tout).
            let mut tooltip: Option<Vec<String>> = None;

            // Barre du haut + ressources en « chips » survolables (infobulle).
            c.fill_rect(0, 0, w, TOP_H, gui::PANEL);
            c.fill_rect(0, TOP_H, w, 1, gui::BORDER);
            c.text(10, 12, &self.stats, 2, gui::TEXT);
            if let Some(res) = self.world.as_ref().and_then(|w| w.nation(self.player)).map(|n| {
                [
                    ("argent", n.money),
                    ("mat", n.materials),
                    ("nour", n.food),
                    ("hab", n.housing),
                    ("infl", n.influence),
                    ("sci", n.knowledge as i64),
                    ("force", n.manpower),
                ]
            }) {
                let mut rx = 14 + gui::text_w(&self.stats, 2) + 18;
                for (key, val) in res {
                    let label = format!("{key} {val}");
                    let cw = gui::text_w(&label, 2) + 10;
                    let hov = (rx..rx + cw).contains(&mx) && (2..TOP_H - 2).contains(&my);
                    c.text(rx + 5, 12, &label, 2, if hov { gui::GOOD } else { gui::TEXT_DIM });
                    if hov {
                        tooltip = Some(vec![resource_tooltip(key).to_string()]);
                    }
                    rx += cw + 4;
                }
            }

            // Barre du bas.
            c.fill_rect(0, h - BOT_H, w, BOT_H, gui::PANEL);
            c.fill_rect(0, h - BOT_H, w, 1, gui::BORDER);
            let spd = speed_label(self.speed);
            let mode_line = if self.replay_mode {
                format!(
                    "REJEU [{spd}]  -  {} / {} commandes  -  tech E{} T{} F{} L{}",
                    self.replay_pos,
                    self.replay_cmds.len(),
                    tech[0],
                    tech[1],
                    tech[2],
                    tech[3]
                )
            } else {
                let mode = if self.spectator { "SPECTATEUR" } else { "JEU" };
                format!(
                    "Mode {mode} [{spd}]  -  outil: {toolname}  -  tech E{} T{} F{} L{}",
                    tech[0], tech[1], tech[2], tech[3]
                )
            };
            c.text(10, h - BOT_H - 22, &mode_line, 2, gui::TEXT_DIM);

            // Boutons (avec état actif pour l'outil / la vitesse courante).
            for (id, b) in &buttons {
                let hover = b.hit(mx, my);
                let active = match id {
                    GameBtn::Tool(t) => *t == self.tool,
                    GameBtn::Speed(s) => *s == self.speed,
                    GameBtn::Cat(c) => *c == self.cat,
                    _ => false,
                };
                b.draw(&mut c, hover, active);
                if hover {
                    let tip = match id {
                        GameBtn::Tool(t) => tool_tooltip(*t),
                        GameBtn::Research(_) => Some(vec![
                            "Recherche".to_string(),
                            "Depense de la science pour monter un palier de tech.".to_string(),
                        ]),
                        _ => None,
                    };
                    if tip.is_some() {
                        tooltip = tip;
                    }
                }
            }

            // Message d'action (succès vert / rejet rouge).
            if !self.last_msg.is_empty() {
                let col = if self.last_msg.starts_with("REJET") {
                    gui::WARN
                } else {
                    gui::GOOD
                };
                let tw = gui::text_w(&self.last_msg, 2);
                c.text(w - 10 - tw, h - BOT_H - 22, &self.last_msg, 2, col);
            }

            // Overlay d'AUDIT du Directeur (invisible au joueur ; --debug-director).
            if self.debug_director {
                let it = self.director.intent();
                let lines = [
                    format!("DIRECTEUR [audit] - {}", self.director_status),
                    format!(
                        "intention: {:?}  i{}  focus {:?}  jusqu'au mois {}",
                        it.stance, it.intensity, it.focus, it.until_turn
                    ),
                    format!("cause publique : {}", it.public_cause),
                    format!("intention cachee: {}", it.hidden_intent),
                ];
                let pw = 600;
                let ph = lines.len() as i32 * 16 + 12;
                let x0d = 10;
                let y0d = TOP_H + 8;
                c.blend_rect(x0d, y0d, pw, ph, gui::PANEL, 230);
                c.rect_outline(x0d, y0d, pw, ph, gui::WARN);
                for (i, l) in lines.iter().enumerate() {
                    c.text(x0d + 8, y0d + 6 + i as i32 * 16, l, 1, gui::TEXT);
                }
            }

            // Panneau de la case inspectée.
            if let (Some((tx, ty)), Some(world)) = (self.selected, self.world.as_ref()) {
                let t = world.tile(tx, ty);
                let owner = t
                    .owner
                    .map(|o| format!("N{o}"))
                    .unwrap_or_else(|| "libre".to_string());
                let bat = t.building.map(building_fr).unwrap_or("aucun");
                let lines = [
                    format!("Case ({tx}, {ty})  -  {:?}", t.biome),
                    format!("proprietaire : {owner}"),
                    format!("batiment     : {bat}"),
                    format!("population   : {:.0}", t.population),
                    format!("developpement: {:.2}", t.development),
                    format!("capacite     : {:.0}", world.capacity_at(tx, ty)),
                    format!("devastation  : {:.2}", t.devastation),
                ];
                let pw = 360;
                let ph = lines.len() as i32 * 20 + 16;
                let px0 = 10;
                let py0 = h - BOT_H - 22 - ph - 8;
                c.blend_rect(px0, py0, pw, ph, gui::PANEL, 235);
                c.rect_outline(px0, py0, pw, ph, gui::BORDER);
                for (i, l) in lines.iter().enumerate() {
                    c.text(px0 + 10, py0 + 8 + i as i32 * 20, l, 1, gui::TEXT);
                }
            }

            // Infobulle (rôle + prix d'un bâtiment, rôle d'une ressource) en dernier.
            if let Some(lines) = &tooltip {
                draw_tooltip(&mut c, mx, my, w, h, lines);
            }
        }
        self.buf = buf;
    }
}

/// Avance le monde d'un tour : Step + Directeur + IA (toutes les nations si spectateur).
fn step_world(world: &mut World, player: u16, nations: u16, spectator: bool) {
    world.apply(Command::Step);
    for c in ai::direct(world, player) {
        world.apply(c);
    }
    for nid in 0..nations {
        if !spectator && nid == player {
            continue;
        }
        for c in ai::plan(world, nid) {
            world.apply(c);
        }
    }
}

/// Nom français d'un bâtiment (les chaînes d'affichage sont en français).
fn building_fr(b: Building) -> &'static str {
    match b {
        Building::City => "Ville",
        Building::Industry => "Industrie",
        Building::Commerce => "Commerce",
        Building::Infrastructure => "Infrastructure",
        Building::Education => "Education",
        Building::Military => "Caserne",
        Building::Farm => "Ferme",
        Building::Port => "Port",
    }
}

/// Nom français d'un type d'unité.
fn unit_fr(k: UnitKind) -> &'static str {
    match k {
        UnitKind::Infantry => "Infanterie",
        UnitKind::Archer => "Archers",
        UnitKind::Cavalry => "Cavalerie",
        UnitKind::Galley => "Galere",
    }
}

/// Rôle (1 ligne) d'un bâtiment, pour l'infobulle.
fn building_role(b: Building) -> &'static str {
    match b {
        Building::City => "Produit de la population ; consomme de la nourriture.",
        Building::Industry => "Produit des materiaux ; pollue lentement la case.",
        Building::Commerce => "Transforme les materiaux en argent + habitation.",
        Building::Infrastructure => "Relie les cases en reseau (main-d'oeuvre commune).",
        Building::Education => "Produit de la science (exige un commerce connecte).",
        Building::Military => "Produit de la force (pour recruter des unites).",
        Building::Farm => "Produit de la nourriture (selon le terrain).",
        Building::Port => "Sur une cote (eau) : force + recrute des navires.",
    }
}

/// Rôle (1 ligne) d'un type d'unité.
fn unit_role(k: UnitKind) -> &'static str {
    match k {
        UnitKind::Infantry => "Robuste, corps a corps. Aucun malus de terrain.",
        UnitKind::Archer => "Distance (portee 2), fragile. Malus en foret.",
        UnitKind::Cavalry => "Rapide et puissante. Malus en terrain accidente.",
        UnitKind::Galley => "Navale (port) : se deplace sur l'eau, transporte 2 unites.",
    }
}

/// Ligne de coût « Cout: X argent[, Y mat][, Z hab] ».
fn cost_line(money: i64, mat: i64, housing: i64) -> String {
    let mut s = format!("Cout: {money} argent");
    if mat > 0 {
        s.push_str(&format!(", {mat} mat"));
    }
    if housing > 0 {
        s.push_str(&format!(", {housing} hab"));
    }
    s
}

/// Infobulle (rôle + coût) d'un outil, si pertinente.
fn tool_tooltip(t: Tool) -> Option<Vec<String>> {
    Some(match t {
        Tool::Build(b) => {
            let (m, mat, h) = sim::build_cost(b);
            vec![
                building_fr(b).to_string(),
                building_role(b).to_string(),
                cost_line(m, mat, h),
            ]
        }
        Tool::Create(k) => {
            let s = sim::unit::unit_stats(k);
            vec![
                unit_fr(k).to_string(),
                unit_role(k).to_string(),
                format!(
                    "Cout: {} argent + {} force (tech Fer {})",
                    s.cost_money, s.cost_force, s.tech_fer
                ),
                format!(
                    "PV {} - degats {} - portee {} - mouvement {}",
                    s.max_hp, s.damage, s.range, s.moves
                ),
            ]
        }
        Tool::Demolish => vec![
            "Demolir".to_string(),
            "Retire le batiment (on peut rebatir autre chose).".to_string(),
            "Rembourse la moitie du cout x l'etat de la case.".to_string(),
        ],
        Tool::Swarm => vec![
            "Etendre".to_string(),
            "Revendique une case voisine (2 clics).".to_string(),
            format!("Coute {} influence ; source >= 1000 hab.", sim::SWARM_INFLUENCE),
        ],
        Tool::Units => vec![
            "Unites".to_string(),
            "Clic = selectionner une unite a soi ;".to_string(),
            "re-clic = deplacer (case vide) ou attaquer (ennemi).".to_string(),
        ],
        Tool::War => vec![
            "Guerre".to_string(),
            "Declare la guerre (clic sur une case ennemie).".to_string(),
        ],
        Tool::Peace => vec![
            "Paix".to_string(),
            "Fait la paix (clic sur une case ennemie).".to_string(),
        ],
        Tool::None => vec![
            "Inspecter".to_string(),
            "Clic sur une case pour l'examiner.".to_string(),
        ],
    })
}

/// Infobulle d'une ressource du HUD (par clé courte).
fn resource_tooltip(key: &str) -> &'static str {
    match key {
        "argent" => "Argent : batir, entretien, recruter des unites.",
        "mat" => "Materiaux : produits par l'industrie ; batir / commerce.",
        "nour" => "Nourriture : nourrit la population (au-dela de la subsistance), sinon famine.",
        "hab" => "Habitation : produite par le commerce ; fonder des villes.",
        "infl" => "Influence : +1/mois ; etendre le territoire.",
        "sci" => "Science : produite par l'education ; payer la recherche.",
        "force" => "Force (manpower) : casernes/ports ; recruter et regenerer les unites.",
        _ => "",
    }
}

/// Dessine une infobulle (boîte sombre) près du curseur, bornée à l'écran.
fn draw_tooltip(c: &mut Canvas, mx: i32, my: i32, w: i32, h: i32, lines: &[String]) {
    if lines.is_empty() {
        return;
    }
    let (pad, lh) = (6, 14);
    let tw = lines.iter().map(|l| gui::text_w(l, 1)).max().unwrap_or(0);
    let bw = tw + pad * 2;
    let bh = lines.len() as i32 * lh + pad * 2;
    let mut x0 = mx + 14;
    let mut y0 = my + 16;
    if x0 + bw > w {
        x0 = (mx - bw - 8).max(0);
    }
    if y0 + bh > h {
        y0 = (my - bh - 8).max(0);
    }
    c.blend_rect(x0, y0, bw, bh, gui::PANEL, 240);
    c.rect_outline(x0, y0, bw, bh, gui::BORDER);
    for (i, l) in lines.iter().enumerate() {
        c.text(x0 + pad, y0 + pad + i as i32 * lh, l, 1, gui::TEXT);
    }
}

/// Traduit le résultat d'une commande joueur en message court (succès ou rejet).
fn feedback(events: &[Event]) -> Option<String> {
    for e in events {
        match e {
            Event::CommandRejected { reason } => return Some(format!("REJET : {reason}")),
            Event::Settled {
                x, y, population, ..
            } => return Some(format!("fonde ({x},{y}) +{population} hab")),
            Event::Swarmed {
                to_x, to_y, moved, ..
            } => return Some(format!("expansion vers ({to_x},{to_y}) +{moved:.0} hab")),
            Event::Researched { branch, tier, .. } => {
                let b = ["Essor", "Terroir", "Fer", "Lien"]
                    .get(*branch as usize)
                    .copied()
                    .unwrap_or("?");
                return Some(format!("{b} palier {tier}"));
            }
            Event::UnitCreated { kind, x, y, .. } => {
                return Some(format!("recrute {} ({x},{y})", unit_fr(*kind)))
            }
            Event::UnitMoved { to_x, to_y, .. } => {
                return Some(format!("unite -> ({to_x},{to_y})"))
            }
            Event::UnitAttacked {
                x,
                y,
                damage,
                killed,
                ..
            } => {
                let k = if *killed { " (detruite)" } else { "" };
                return Some(format!("attaque ({x},{y}) -{damage} PV{k}"));
            }
            Event::UnitDestroyed { x, y, .. } => {
                return Some(format!("unite detruite ({x},{y})"))
            }
            Event::Embarked { .. } => return Some("embarque sur la galere".to_string()),
            Event::Disembarked { kind, x, y, .. } => {
                return Some(format!("debarque {} ({x},{y})", unit_fr(*kind)))
            }
            Event::Capitulation {
                winner,
                loser,
                tiles,
                ..
            } => {
                return Some(format!(
                    "CAPITULATION : N{winner} annexe {tiles} cases de N{loser}"
                ))
            }
            Event::WarDeclared { target, .. } => {
                return Some(format!("GUERRE declaree a N{target}"))
            }
            Event::PeaceMade { target, .. } => return Some(format!("paix avec N{target}")),
            Event::Built { x, y, building, .. } => {
                return Some(format!("bati {} ({x},{y})", building_fr(*building)))
            }
            Event::Demolished {
                building, refund, ..
            } => {
                return Some(format!("demoli {} (+{refund} argent)", building_fr(*building)))
            }
            _ => {}
        }
    }
    None
}

/// Assombrit une couleur ARGB en multipliant chaque canal par `f`/255.
fn scale_rgb(p: u32, f: u32) -> u32 {
    let r = (((p >> 16) & 255) * f / 255) & 255;
    let g = (((p >> 8) & 255) * f / 255) & 255;
    let b = ((p & 255) * f / 255) & 255;
    (r << 16) | (g << 8) | b
}

/// Taille de l'écran principal (Win32) ; repli raisonnable sinon.
fn screen_size() -> (usize, usize) {
    #[cfg(windows)]
    unsafe {
        use winapi::um::winuser::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
        let w = GetSystemMetrics(SM_CXSCREEN);
        let h = GetSystemMetrics(SM_CYSCREEN);
        if w > 0 && h > 0 {
            return (w as usize, h as usize);
        }
    }
    (1600, 900)
}

/// Zone de travail (écran moins la barre des tâches) : (x, y, largeur, hauteur).
/// Utilisée pour le plein écran afin que rien ne soit caché sous la barre.
fn work_area() -> (i32, i32, i32, i32) {
    #[cfg(windows)]
    unsafe {
        use winapi::shared::windef::RECT;
        use winapi::um::winuser::{SystemParametersInfoW, SPI_GETWORKAREA};
        let mut r: RECT = std::mem::zeroed();
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut r as *mut RECT as *mut _, 0) != 0 {
            return (r.left, r.top, r.right - r.left, r.bottom - r.top);
        }
    }
    let (w, h) = screen_size();
    (0, 0, w as i32, h as i32)
}

/// Arguments de la ligne de commande.
struct Args {
    seed: u64,
    nations: u16,
    player: u16,
    pre_turns: usize,
    px: u32,
    fullscreen: bool,
    spectator: bool,
    headless: bool,
    shot: Option<String>,
    screen: String,
    audit: bool,
    out: Option<String>,
    record: Option<String>,
    replay: Option<String>,
    debug_director: bool,
    /// Position de survol simulée pour la capture headless (infobulles).
    hover: Option<(i32, i32)>,
}

impl Args {
    fn parse() -> Self {
        let mut a = Args {
            seed: 2026,
            nations: 8,
            player: 0,
            pre_turns: 0,
            px: 14,
            fullscreen: false,
            spectator: false,
            headless: false,
            shot: None,
            screen: "game".to_string(),
            audit: false,
            out: None,
            record: None,
            replay: None,
            debug_director: false,
            hover: None,
        };
        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "--seed" => set(&mut a.seed, it.next()),
                "--nations" => set(&mut a.nations, it.next()),
                "--player" => set(&mut a.player, it.next()),
                "--pre-turns" => set(&mut a.pre_turns, it.next()),
                "--px" => set(&mut a.px, it.next()),
                "--fullscreen" => a.fullscreen = true,
                "--spectator" => a.spectator = true,
                "--headless" => a.headless = true,
                "--shot" => a.shot = it.next(),
                "--screen" => {
                    if let Some(v) = it.next() {
                        a.screen = v;
                    }
                }
                "--audit" => a.audit = true,
                "--out" => a.out = it.next(),
                "--record" => a.record = it.next(),
                "--replay" => a.replay = it.next(),
                "--debug-director" => a.debug_director = true,
                "--hover" => {
                    a.hover = it.next().and_then(|v| {
                        let p: Vec<i32> = v.split(',').filter_map(|s| s.parse().ok()).collect();
                        (p.len() == 2).then_some((p[0], p[1]))
                    });
                }
                other => eprintln!("argument ignoré : {other}"),
            }
        }
        let _ = a.player; // réservé (joueur = nation 0 pour l'instant)
        a
    }
}

/// Affecte `dst` si `v` se parse, sinon le laisse inchangé.
fn set<T: std::str::FromStr>(dst: &mut T, v: Option<String>) {
    if let Some(x) = v.and_then(|v| v.parse().ok()) {
        *dst = x;
    }
}
