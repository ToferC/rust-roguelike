use tcod::colors::*;
use tcod::console::*;
use tcod::input::Key;
use tcod::input::KeyCode::*;

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const LIMIT_FPS: i32 = 20;

struct Tcod {
    root: Root,
    con: Offscreen,
}

struct Object {
    x: i32,
    y: i32,
    character: char,
    color: Color,
}

impl Object {
    pub fn new(x: i32, y: i32, character: char, color: Color) -> Self {
        Object { x, y, character, color }
    }

    pub fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.character, BackgroundFlag::None);
    }
}

fn handle_keys(tcod: &mut Tcod, player: &mut Object) -> bool {
    
    let key = tcod.root.wait_for_keypress(true);

    match key {
        // Alt+Enter: toggle fullscreen
        Key {
            code: Enter,
            alt: true,
            ..
        } => {
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
        },
        Key { code: Escape, ..} => return true, // exit game

        // movement keys
        Key { code: Up, ..} => player.move_by(0, -1),
        Key { code: Down, ..} => player.move_by(0, 1),
        Key { code: Left, ..} => player.move_by(-1, 0),
        Key { code: Right, ..} => player.move_by(1, 0),

        _ => {}
    }
    false
}

fn main() {

    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();

        
    let con = Offscreen::new(SCREEN_WIDTH, SCREEN_HEIGHT);

        
    let mut tcod = Tcod { root, con };

    // define player object
    let player = Object::new(SCREEN_WIDTH / 2, SCREEN_HEIGHT / 2, '@', WHITE);

    // create NPC
    let npc = Object::new(SCREEN_WIDTH / 2 - 5, SCREEN_HEIGHT / 2 - 5, '@', YELLOW);

    // list of mutable objects
    let mut objects = [player, npc];

    // Game Loop
    while !tcod.root.window_closed() {
        // clear contents of previous screen
        tcod.con.clear();

        for object in &objects {
            object.draw(&mut tcod.con)
        }

        blit(
            &tcod.con,
            (0, 0),
            (SCREEN_WIDTH, SCREEN_HEIGHT),
            &mut tcod.root,
            (0, 0),
            1.0,
            1.0,
        );
        
        tcod.root.flush();

        // handle keys and exit game if needed
        let player = &mut objects[0];
        let exit = handle_keys(&mut tcod, player);
        if exit {
            break
        }
    }

}
