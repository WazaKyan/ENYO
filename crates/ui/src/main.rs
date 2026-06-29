//! Fenêtre de jeu ENYO (minifb) — menu, paramètres, jeu tour par tour.
//!
//! Interface dessinée dans le framebuffer (boutons + HUD, police bitmap `gui`).
//! La fenêtre s'ajuste à l'écran (fenêtré ~90 % ou plein écran sans bordure).
//!
//! Jeu : Espace / bouton = fin de tour · clic = inspecter / agir · F = Fonder ·
//! E = Essaimer (2 clics) · N = inspecter · 1-4 = recherche · WASD = bouger ·
//! molette = zoom · Échap = retour menu.
//! Spectateur : 0/1/2 = pause / ×1 / ×2 (le monde joue seul).
//!
//! Mode agent : `--headless --shot f.png [--screen menu|settings|game]`
//! rend exactement l'écran demandé en PNG (vérifiable sans ouvrir la fenêtre).

mod gui;

use gui::{Button, Canvas};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use proto::{Command, Event};
use sim::World;

const TOP_H: i32 = 40;
const BOT_H: i32 = 52;

#[derive(PartialEq, Clone, Copy)]
enum Tool {
    None,
    Found,
    Swarm,
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
    EndTurn,
    Tool(Tool),
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
    fn key(k: Key) -> Self {
        Input {
            pressed: vec![k],
            down: vec![k],
            mx: -1,
            my: -1,
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
const WATCH: [Key; 18] = [
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
    tool: Tool,
    selected: Option<(u32, u32)>,
    swarm_src: Option<(u32, u32)>,
    speed: u32,
    frame: u64,
    last_msg: String,
    stats: String,
    stats_dirty: bool,
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
    let mut mouse_was_down = false;

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
fn run_headless(args: &Args) {
    let mut app = App::new(args);
    let (w, h) = (1280usize, 800usize);
    app.buf = vec![gui::BG; w * h];
    let (wi, hi) = (w as i32, h as i32);
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
            app.selected = Some((app.cam_x, app.cam_y)); // aperçu du panneau d'inspection
            app.last_msg = "essaimage vers (402,250) +500 hab".to_string();
            app.draw_game(wi, hi, -1, -1);
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

    // Quelques fins de tour (le monde évolue).
    for i in 0..3 {
        a.step(&mut app, &Input::key(Key::Space), &format!("jeu_tour{}", i + 1));
    }

    // Outil Fonder + clic sur la carte.
    let p = center_of(&app.game_buttons(w, h), GameBtn::Tool(Tool::Found));
    a.step(&mut app, &Input::click_at(p.0, p.1), "outil_fonder");
    a.step(&mut app, &Input::click_at(map_pt.0, map_pt.1), "fonder_case");

    // Recherche (montre le succès OU le rejet « savoir insuffisant »).
    let p = center_of(&app.game_buttons(w, h), GameBtn::Research(0));
    a.step(&mut app, &Input::click_at(p.0, p.1), "recherche_essor");

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
            px: args.px,
            tool: Tool::None,
            selected: None,
            swarm_src: None,
            speed: 0,
            frame: 0,
            last_msg: String::new(),
            stats: String::new(),
            stats_dirty: true,
        }
    }

    // ---- Cycle de partie -------------------------------------------------

    fn start_game(&mut self, spectator: bool) {
        let mut world = World::new(self.config.seed, 800, 500);
        for c in ai::spawn_nations(&world, self.config.nations) {
            world.apply(c);
        }
        for _ in 0..self.config.pre_turns {
            step_world(&mut world, self.player, self.config.nations, true);
        }
        let (cx, cy) = render::nation_bbox(&world, self.player, 0)
            .map(|(x, y, w, h)| (x + w / 2, y + h / 2))
            .unwrap_or((world.width / 2, world.height / 2));
        self.cam_x = cx;
        self.cam_y = cy;
        self.px = self.config.px;
        self.world = Some(world);
        self.spectator = spectator;
        self.tool = Tool::None;
        self.selected = None;
        self.swarm_src = None;
        self.speed = if spectator { 1 } else { 0 };
        self.last_msg.clear();
        self.stats_dirty = true;
        self.screen = Screen::Game;
    }

    fn end_turn(&mut self) {
        let (player, nations, spec) = (self.player, self.config.nations, self.spectator);
        if let Some(world) = self.world.as_mut() {
            step_world(world, player, nations, spec);
        }
        self.stats_dirty = true;
    }

    fn ensure_menu_world(&mut self) {
        if self.menu_world.is_none() {
            let mut world = World::new(self.config.seed, 800, 500);
            for c in ai::spawn_nations(&world, self.config.nations) {
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
        let kn = world.nation(self.player).map(|n| n.knowledge).unwrap_or(0.0);
        let year = world.turn / 12;
        let month = world.turn % 12 + 1;
        self.stats = format!(
            "An {year} M{month:02}   N{}   {pop:.0} hab   {tiles} cases   {prov} prov   savoir {kn:.0}",
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

    fn handle_game(&mut self, input: &Input, w: i32, h: i32) {
        self.frame = self.frame.wrapping_add(1);
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
        // Outils.
        if input.key_pressed(Key::F) {
            self.set_tool(Tool::Found);
        }
        if input.key_pressed(Key::E) {
            self.set_tool(Tool::Swarm);
        }
        if input.key_pressed(Key::N) {
            self.set_tool(Tool::None);
        }
        // Fin de tour.
        if input.key_pressed(Key::Space) {
            self.end_turn();
        }
        // Chiffres : recherche (joueur) ou vitesse (spectateur).
        if self.spectator {
            for (k, s) in [(Key::Key0, 0u32), (Key::Key1, 1), (Key::Key2, 2)] {
                if input.key_pressed(k) {
                    self.speed = s;
                }
            }
        } else {
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

        // Auto-tour en spectateur.
        if self.spectator && self.speed > 0 {
            let interval: u64 = if self.speed >= 2 { 12 } else { 28 };
            if self.frame.is_multiple_of(interval) {
                self.end_turn();
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
        self.swarm_src = None;
    }

    fn research(&mut self, branch: u8) {
        let player = self.player;
        if let Some(world) = self.world.as_mut() {
            let ev = world.apply(Command::Research {
                nation: player,
                branch,
            });
            if let Some(m) = feedback(&ev) {
                self.last_msg = m;
            }
        }
        self.stats_dirty = true;
    }

    fn do_game_btn(&mut self, id: GameBtn) {
        match id {
            GameBtn::Menu => self.screen = Screen::Menu,
            GameBtn::EndTurn => self.end_turn(),
            GameBtn::Tool(t) => self.set_tool(t),
            GameBtn::Research(b) => self.research(b),
            GameBtn::Speed(s) => self.speed = s,
        }
    }

    fn map_click(&mut self, mx: i32, my: i32, w: i32, h: i32) {
        let Some(world) = self.world.as_mut() else {
            return;
        };
        let (x0, y0, _, _, pxe) =
            render::viewport_rect(world, self.cam_x, self.cam_y, self.px, w as u32, h as u32);
        let tx = x0 + (mx as u32) / pxe;
        let ty = y0 + (my as u32) / pxe;
        if tx >= world.width || ty >= world.height {
            return;
        }
        self.selected = Some((tx, ty));
        let player = self.player;
        match self.tool {
            Tool::Found => {
                let ev = world.apply(Command::Settle {
                    x: tx,
                    y: ty,
                    nation: player,
                    population: 300,
                });
                if let Some(m) = feedback(&ev) {
                    self.last_msg = m;
                }
                self.stats_dirty = true;
            }
            Tool::Swarm => {
                if let Some((sx, sy)) = self.swarm_src.take() {
                    let ev = world.apply(Command::Swarm {
                        from_x: sx,
                        from_y: sy,
                        to_x: tx,
                        to_y: ty,
                    });
                    if let Some(m) = feedback(&ev) {
                        self.last_msg = m;
                    }
                    self.stats_dirty = true;
                } else {
                    self.swarm_src = Some((tx, ty));
                    self.last_msg = format!("source ({tx},{ty}) - clique la cible");
                }
            }
            Tool::None => {}
        }
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
        let turn_w = gui::text_w("Fin de tour", 2) + 20;
        let mxn = w - pad - menu_w;
        v.push((GameBtn::Menu, Button::new(mxn, 6, menu_w, bh, "Menu")));
        let mxt = mxn - pad - turn_w;
        v.push((GameBtn::EndTurn, Button::new(mxt, 6, turn_w, bh, "Fin de tour")));

        // Bas : outils puis recherche (ou vitesse en spectateur).
        let by = h - BOT_H + 12;
        let tbh = 28;
        let mut x = pad;
        for (lbl, t) in [
            ("Inspecter", Tool::None),
            ("Fonder", Tool::Found),
            ("Essaimer", Tool::Swarm),
        ] {
            let bw = gui::text_w(lbl, 2) + 18;
            v.push((GameBtn::Tool(t), Button::new(x, by, bw, tbh, lbl)));
            x += bw + 6;
        }
        x += 24;
        if self.spectator {
            for (lbl, s) in [("Pause", 0u32), ("x1", 1), ("x2", 2)] {
                let bw = gui::text_w(lbl, 2) + 18;
                v.push((GameBtn::Speed(s), Button::new(x, by, bw, tbh, lbl)));
                x += bw + 6;
            }
        } else {
            for (i, lbl) in ["Essor", "Terroir", "Fer", "Lien"].iter().enumerate() {
                let bw = gui::text_w(lbl, 2) + 18;
                v.push((GameBtn::Research(i as u8), Button::new(x, by, bw, tbh, *lbl)));
                x += bw + 6;
            }
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
            Tool::Found => "Fonder",
            Tool::Swarm => "Essaimer",
        };
        let tech = self
            .world
            .as_ref()
            .and_then(|w| w.nation(self.player))
            .map(|n| n.tech)
            .unwrap_or_default();
        {
            let mut c = Canvas::new(&mut buf, uw, uh);
            // Surbrillance de la case sélectionnée / source d'essaimage.
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
            mark(&mut c, self.swarm_src, gui::GOOD);
            mark(&mut c, self.selected, gui::TEXT);

            // Barre du haut.
            c.fill_rect(0, 0, w, TOP_H, gui::PANEL);
            c.fill_rect(0, TOP_H, w, 1, gui::BORDER);
            c.text(10, 12, &self.stats, 2, gui::TEXT);

            // Barre du bas.
            c.fill_rect(0, h - BOT_H, w, BOT_H, gui::PANEL);
            c.fill_rect(0, h - BOT_H, w, 1, gui::BORDER);
            let mode = if self.spectator {
                "SPECTATEUR"
            } else {
                "JEU"
            };
            c.text(
                10,
                h - BOT_H - 22,
                &format!(
                    "Mode {mode}  -  outil: {toolname}  -  tech E{} T{} F{} L{}",
                    tech[0], tech[1], tech[2], tech[3]
                ),
                2,
                gui::TEXT_DIM,
            );

            // Boutons (avec état actif pour l'outil / la vitesse courante).
            for (id, b) in &buttons {
                let hover = b.hit(mx, my);
                let active = match id {
                    GameBtn::Tool(t) => *t == self.tool,
                    GameBtn::Speed(s) => self.spectator && *s == self.speed,
                    _ => false,
                };
                b.draw(&mut c, hover, active);
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

            // Panneau de la case inspectée.
            if let (Some((tx, ty)), Some(world)) = (self.selected, self.world.as_ref()) {
                let t = world.tile(tx, ty);
                let owner = t
                    .owner
                    .map(|o| format!("N{o}"))
                    .unwrap_or_else(|| "libre".to_string());
                let lines = [
                    format!("Case ({tx}, {ty})  -  {:?}", t.biome),
                    format!("proprietaire : {owner}"),
                    format!("population   : {:.0}", t.population),
                    format!("developpement: {:.2}", t.development),
                    format!("capacite     : {:.0}", world.capacity_at(tx, ty)),
                    format!("force        : {:.0}", t.force),
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
            } => return Some(format!("essaimage vers ({to_x},{to_y}) +{moved:.0} hab")),
            Event::Researched { branch, tier, .. } => {
                let b = ["Essor", "Terroir", "Fer", "Lien"]
                    .get(*branch as usize)
                    .copied()
                    .unwrap_or("?");
                return Some(format!("{b} palier {tier}"));
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
