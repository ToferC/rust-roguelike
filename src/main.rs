use tcod::colors::*;
use tcod::console::*;
use tcod::input::{self, Event, Key, Mouse};
use tcod::input::KeyCode::*;
use tcod::map::{FovAlgorithm, Map as FovMap};

use std::cmp;
use rand::Rng;
use rand::prelude::*;
use rand::distributions::{WeightedIndex};
use std::io::BufReader;

use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

use serde::{Deserialize, Serialize};

use rodio::Sink;

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

const SCREEN_LEVEL_WIDTH: i32 = 40;
const CHARACTER_SCREEN_WIDTH: i32 = 30;

const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

// Items
const HEAL_AMOUNT: i32 = 40;

const LIGHTNING_DAMAGE: i32 = 40;
const LIGHTNING_RANGE: i32 = 5;

const CONFUSE_NUM_TURNS: i32 = 10;
const CONFUSE_RANGE: i32 = 8;

const FIREBALL_DAMAGE: i32 = 25;
const FIREBALL_RADIUS: i32 = 3;

// Player

const PLAYER: usize = 0;

const LEVEL_UP_BASE: i32 = 200;
const LEVEL_UP_FACTOR: i32 = 150;

// Field of view

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic;
const FOV_LIGHT_WALLS: bool = false;
const TORCH_RADIUS: i32 = 10;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_LIGHT_WALL: Color = Color {
    r: 130,
    g: 110,
    b: 50,
};
const COLOR_DARK_GROUND: Color = Color {
    r: 50,
    g: 50,
    b: 150
};
const COLOR_LIGHT_GROUND: Color = Color {
    r: 200,
    g: 180,
    b: 50,
};

const LIMIT_FPS: i32 = 20;

// sizes and coordinates for the GUI
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;

const INVENTORY_WIDTH: i32 = 50;

#[derive(Serialize, Deserialize)]
struct Messages {
    messages: Vec<(String, Color)>,
}

impl Messages {
    pub fn new() -> Self {
        Self { messages: vec![] }
    }

    /// add new message as a tuple with text and color
    pub fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        self.messages.push((message.into(), color));
    }

    /// Create DoubleEndedIterator over the messages
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(String, Color)> {
        self.messages.iter()
    }
}

struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    key: Key,
    mouse: Mouse,
    sink: rodio::Sink,
}

// Tiles
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Tile {
    blocked: bool,
    block_sight: bool,
    explored: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            block_sight: false,
            explored: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            block_sight: true,
            explored: false,
        }
    }
}

// Rectangles
#[derive(Clone, Copy, Debug)]
struct Rect {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect {
            x1: x,
            y1: y,
            x2: x + w,
            y2: y + h,
        }
    }

    pub fn center(&self) -> (i32, i32) {
        let center_x = (self.x1 + self.x2) / 2;
        let center_y = (self.y1 + self.y2) / 2;
        (center_x, center_y)
    }

    pub fn intersects_with(&self, other: &Rect) -> bool {
        // returns true if this rectangle intersects with another one
        (self.x1 <= other.x2)
            && (self.x2 >= other.x1)
            && (self.y1 <= other.y2)
            && (self.y2 >= other.y1)
    } 
}

// Dungeon creation functions
fn create_room( room: Rect, map: &mut Map) {
    // go through tiles in the rectangle and make them passable
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    // horizontal tunnel. min() and max() are used in case x1 > x2
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1 ) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    // vertical tunnel as above
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

// Map
type Map = Vec<Vec<Tile>>;

#[derive(Serialize, Deserialize)]
struct Game {
    map: Map,
    messages: Messages,
    inventory: Vec<Object>,
    dungeon_level: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTaketurn,
    Exit,
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    // first test the map tile
    if map[x as usize][y as usize].blocked {
        return true
    }
    // now check for blocking objects
    objects
        .iter()
        .any(|object| object.blocks && object.pos() == (x, y))
}

fn make_map(objects: &mut Vec<Object>, level: u32) -> Map {

    // fill with blocked tiles
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    // Player is the first element, remove everything else
    // NOTE: only works when the player is the first object
    assert_eq!(&objects[PLAYER] as *const _, &objects[0] as *const _);
    objects.truncate(1);
    
    let mut rooms = vec![];

    for _ in 0..MAX_ROOMS {
        // random width and height
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        // random position without going beyond map boundaries
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let new_room = Rect::new(x, y, w, h);

        // run through other rooms and see if they intersect with this one
        let failed = rooms
            .iter()
            .any(|other_room| new_room.intersects_with(other_room));

        if !failed {
            // means so intersections, so the room is valid
            create_room(new_room, &mut map);

            place_objects(new_room, &map, objects, level);

            let (new_x, new_y) = new_room.center();

            if rooms.is_empty() {
                objects[PLAYER].set_pos(new_x, new_y);
            } else {
                // all rooms after the first
                // connect to previous room with a tunnel

                // centre coordinates of previous room
                let (prev_x, prev_y) = rooms[rooms.len() - 1].center();

                if rand::random() {
                    // first move horizontally, then vertically
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    create_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    // first move horizontally, then vertically
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    create_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            }
            // append new room to list
            rooms.push(new_room);
        }
    }


    let room1 = Rect::new(20, 15, 10, 15);
    let room2 = Rect::new(50, 15, 10, 15);

    create_room(room1, &mut map);
    create_room(room2, &mut map);

    create_h_tunnel(25, 55, 23, &mut map);

    // create stairs at the center of the last room
    let (last_room_x, last_room_y) = rooms[rooms.len() - 1].center();
    let mut stairs = Object::new(last_room_x, last_room_y, '<', WHITE, "stairs".to_string(), false);
    stairs.always_visible = true;
    objects.push(stairs);

    //map[30][22] = Tile::wall();
    //map[50][22] = Tile::wall();

    map
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Object {
    x: i32,
    y: i32,
    glyph: char,
    color: Color,
    name: String,
    blocks: bool,
    alive: bool,
    fighter: Option<Fighter>,
    ai: Option<AI>,
    item: Option<Item>,
    always_visible: bool,
    level: i32,
    equipment: Option<Equipment>,
}

impl Object {
    pub fn new(x: i32, y: i32, glyph: char, color: Color, name: String, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            glyph: glyph,
            color: color,
            name: name,
            blocks: blocks,
            alive: false,
            fighter: None,
            ai: None,
            item: None,
            always_visible: false,
            level: 1,
            equipment: None,
        }
    }

    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.glyph, BackgroundFlag::None);
    }

    pub fn pos(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// return the distance to another object
    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx.pow(2) + dy.pow(2)) as f32).sqrt()
    }

    /// return distance to a position
    pub fn distance(&self, x: i32, y: i32) -> f32 {
        (((x - self.x).pow(2) + (y - self.y).pow(2)) as f32).sqrt()
    }

    // Combat
    pub fn take_damage(&mut self, damage: i32, game: &mut Game) -> Option<i32> {
        // apply damage if possible
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp -= damage;
            }
        }

        // check for death, call the death function
        if let Some(fighter) = self.fighter {
            if fighter.hp <= 0 {
                self.alive = false;
                fighter.on_death.callback(self, game);
                return Some(fighter.xp);
            }
        }
        None
    }

    pub fn heal(&mut self, amount: i32, game: &Game) {
        // heal damage if possible
        let max_hp = self.max_hp(game);

        if let Some(ref mut fighter) = self.fighter {
            fighter.hp += amount;
            if fighter.hp > max_hp {
                fighter.hp = max_hp;
            }
        }
    }

    pub fn attack(&mut self, target: &mut Object, game: &mut Game) {
        // a simple formula for attack damage
        let damage = self.power(game) - target.defense(game);
        if damage > 0 {
            // make the target take damage
            game.messages.add(
                format!(
                "{} attacks {} for {} hit points.",
                self.name, target.name, damage
            ), ORANGE);
            if let Some(xp) = target.take_damage(damage, game) {
                // yield experience to player if target killed
                self.fighter.as_mut().unwrap().xp += xp;
            };
        } else {
            game.messages.add(format!(
                "{} attacks {}, but it has no affect!",
                self.name, target.name
            ), GREEN);
        }
    }

    pub fn ranged_attack(&mut self, target: &mut Object, game: &mut Game, range: f32) {

        let (x, y) = target.pos();

        // confirm target is in range
        if self.distance(x, y) <= range {
            // a simple formula for attack damage
            let damage = self.power(game) - target.defense(game);
            if damage > 0 {
                // make the target take damage
                game.messages.add(
                    format!(
                    "{} shoots {} for {} hit points.",
                    self.name, target.name, damage
                ), ORANGE);
                if let Some(xp) = target.take_damage(damage, game) {
                    // yield experience to player if target killed
                    self.fighter.as_mut().unwrap().xp += xp;
                };
            } else {
                game.messages.add(format!(
                    "{} shoots {}, but it has no affect!",
                    self.name, target.name
                ), GREEN);
            }
        } else {
            game.messages.add(format!(
                "{} shoots at {}, but the target is out of range!",
                self.name, target.name
            ), GREEN);
        }
    }

    /// Equip object and show a message about it
    pub fn equip(&mut self, messages: &mut Messages) {
        if self.item.is_none() {
            messages.add(
                format!("Can't equip {:?} because it's not an Item.", self),
                RED,
            );
            return;
        };
        if let Some(ref mut equipment) = self.equipment {
            if !equipment.equipped {
                equipment.equipped = true;
                messages.add(
                    format!("Equipped {} on {:?}.", self.name, equipment.slot),
                    LIGHT_GREEN,
                );
            }
        } else {
            messages.add(
                format!("Can't equp {:?} because it;s not an Equipment.", self),
                RED,
            );
        }
    }

    /// Dequip object and show a message about it
    pub fn dequip(&mut self, messages: &mut Messages) {
        if self.item.is_none() {
            messages.add(
                format!("Can't dequip {:?} because it's not an item.", self),
                RED,
            );
            return;
        };
        if let Some(ref mut equipment) = self.equipment {
            if equipment.equipped {
                equipment.equipped = false;
                messages.add(
                    format!("Dequipped {} from {:?}.", self.name, equipment.slot),
                    LIGHT_YELLOW,
                );
            }
        } else {
            messages.add(
                format!("Can't dequip {:?} because it's not an Equipment.", self),
                RED,
            );
        }
    }

    pub fn power(&self, game: &Game) -> i32 {
        let base_power = self.fighter.map_or(0, |f| f.base_power);
        let mut bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.power_bonus)
            .sum();

        // add NPC damage by 1 / 4 levels of the dungeon to keep things interesting
        if self.name != "player" {
            bonus += game.dungeon_level as i32 / 4;
        }

        base_power + bonus
    }

    pub fn defense(&self, game: &Game) -> i32 {
        let base_defense = self.fighter.map_or(0, |f| f.base_defense);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.defense_bonus)
            .sum();

        base_defense + bonus
    }

    pub fn max_hp(&self, game: &Game) -> i32 {
        let base_max_hp = self.fighter.map_or(0, |f| f.base_max_hp);
        let bonus: i32 = self
            .get_all_equipped(game)
            .iter()
            .map(|e| e.max_hp_bonus)
            .sum();
        base_max_hp + bonus
    }

    pub fn get_all_equipped(&self, game: &Game) -> Vec<Equipment> {
        if self.name == "player" {
            game.inventory
                .iter()
                .filter(|item| item.equipment.map_or(false, |e| e.equipped))
                .map(|item| item.equipment.unwrap())
                .collect()
        } else {
            vec![] // other items have no equipment
        }
    }

}

fn level_up(tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) {
    let player = &mut objects[PLAYER];
    let level_up_xp = LEVEL_UP_BASE + player.level * LEVEL_UP_FACTOR;
    // see if the player's xp is enough to level up
    if player.fighter.as_ref().map_or(0, |f| f.xp) >= level_up_xp {
        // it is! Level up
        player.level += 1;
        game.messages.add(
            format!(
                "Your battle skills grow stronger! You have reached level {}!",
                player.level,
            ),
            YELLOW,
        );
        // Level up stats
        let fighter = player.fighter.as_mut().unwrap();
        let mut choice = None;
        while choice.is_none() {
            // Keep asking until a choice is made
            choice = menu(
                "Level up! Chose a stat to raise:\n",
                &[
                    format!("Constitution (+20 HP, from {})", fighter.base_max_hp),
                    format!("Strength (+1 attack, from {})", fighter.base_power),
                    format!("Agility (+1 defense, from {})", fighter.base_defense),
                ],
                SCREEN_LEVEL_WIDTH,
                &mut tcod.root,
            );
        }
        fighter.xp -= level_up_xp;
        match choice.unwrap() {
            0 => {
                fighter.base_max_hp += 20;
                fighter.hp += 20;
            }
            1 => {
                fighter.base_power += 1;
            }
            2 => {
                fighter.base_defense += 1;
            }
            _ => unreachable!(),
        }
    }
}

fn player_death(player: &mut Object, game: &mut Game) {
    // the game ended
    game.messages.add("You died!", RED);
    // for addd effect, transform the player into a corpse!
    player.glyph = '%';
    player.color = DARK_RED;
}

fn monster_death(monster: &mut Object, game: &mut Game) {
    // transform it into a nasty corpse. It doesn't block, can't be attacked
    // and doesn't move
    game.messages.add(
        format!("{} is dead! You gain {} xp!", monster.name, monster.fighter.unwrap().xp),
        ORANGE);
    monster.glyph = '%';
    monster.color = DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

// combat related properties
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Fighter {
    base_max_hp: i32,
    hp: i32,
    base_defense: i32,
    base_power: i32,
    xp: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum DeathCallback {
    Player,
    Monster,
}

impl DeathCallback {
    fn callback(self, object: &mut Object, game: &mut Game) {
        use DeathCallback::*;
        let callback = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object, game);
    }
}

// Inventory
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
enum Item {
    Heal,
    Lightning,
    Confuse,
    Fireball,
    Sword,
    Shield,
    Helmet,
    Bow,
}

enum UseResult {
    UsedUp,
    Cancelled,
    UsedAndKept,
    UseCharge,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
/// An object that can be equipped, yielding bonuses
struct Equipment {
    slot: Slot,
    equipped: bool,
    power_bonus: i32,
    defense_bonus: i32,
    max_hp_bonus: i32,
    range: i32,
    damage: i32,
    charges: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
enum Slot {
    LeftHand,
    RightHand,
    Head,
    Back,
}

impl std::fmt::Display for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Slot::LeftHand => write!(f, "left hand"),
            Slot::RightHand => write!(f, "right hand"),
            Slot::Head => write!(f, "head"),
            Slot::Back => write!(f, "back"),
        }
    }
}

fn get_equipped_in_slot(slot: Slot, inventory: &[Object]) -> Option<usize> {
    for (inventory_id, item) in inventory.iter().enumerate() {
        if item
            .equipment
            .as_ref()
            .map_or(false, |e| e.equipped && e.slot == slot)
            {
                return Some(inventory_id)
            }
    }
    None
}

/// Add to player inventory and remove from map
fn pick_item_up(object_id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    if game.inventory.len() >= 26 {
        game.messages.add(
            format!(
                "Your inventory is full. Cannot pick up {}",
                objects[object_id].name
            ),
            RED,
        );
    } else {
        let item = objects.swap_remove(object_id);
        game.messages.add(
            format!("You picked up a {}!", item.name), GREEN);
        let index = game.inventory.len();
        let slot = item.equipment.map(|e| e.slot);
        game.inventory.push(item);

        // automatically equip, if the corresponding equipment slot is unused
        if let Some(slot) = slot {
            if get_equipped_in_slot(slot, &game.inventory).is_none() {
                game.inventory[index].equip(&mut game.messages);
            }
        }
    }
}

// Use items
fn use_item(inventory_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) {
    use Item::*;

    // just call use_function if it is defined
    if let Some(item) = game.inventory[inventory_id].item {
        let on_use = match item {
            Heal => cast_heal,
            Lightning => cast_lightning,
            Confuse => cast_confuse,
            Fireball => cast_fireball,
            Sword => toggle_equipment,
            Shield => toggle_equipment,
            Helmet => toggle_equipment,
            Bow => player_ranged_attack,
        };
        match on_use(inventory_id, tcod, game, objects) {
            UseResult::UsedUp => {
                // destroy after use, unless cancelled
                game.inventory.remove(inventory_id);
            }
            UseResult::UsedAndKept => {} // do nothing
            UseResult::UseCharge => {
                if let Some(equip) = &mut game.inventory[inventory_id].equipment {
                    equip.charges -= 1;
                }
            }
            UseResult::Cancelled => {
                game.messages.add("Cancelled", WHITE);
            }
        }
    } else {
        game.messages.add(
            format!("The {} cannot be used.", game.inventory[inventory_id].name),
            WHITE,
        );
    }
}

fn toggle_equipment(
    inventory_id: usize,
    _tcod: &mut Tcod,
    game: &mut Game,
    _objects: &mut [Object],
) -> UseResult {
    
    let equipment = match game.inventory[inventory_id].equipment {
        Some(equipment) => equipment,
        None => return UseResult::Cancelled,
    };

    // if the slot is already being used, dequip whatever is ther first
    if let Some(current) = get_equipped_in_slot(equipment.slot, &game.inventory) {
        game.inventory[current].dequip(&mut game.messages);
    }

    if equipment.equipped {
        game.inventory[inventory_id].dequip(&mut game.messages);
    } else {
        game.inventory[inventory_id].equip(&mut game.messages);
    }
    UseResult::UsedAndKept
}

fn drop_item(inventory_id: usize, game: &mut Game, objects: &mut Vec<Object>) {
    let mut item = game.inventory.remove(inventory_id);

    if item.equipment.is_some() {
        item.dequip(&mut game.messages);
    }

    item.set_pos(objects[PLAYER].x, objects[PLAYER].y);
    game.messages.add(
        format!(
            "You dropped a {}", item.name,
        ), YELLOW
    );
    objects.push(item);
}

fn cast_heal(
    _inventory_id: usize,
    _tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],

) -> UseResult {
    // heal the player
    let player = &mut objects[PLAYER];
    if let Some(fighter) = player.fighter {
        if fighter.hp == player.max_hp(game) {
            game.messages.add("You are already at full health.", YELLOW);
            return UseResult::Cancelled
        }
        game.messages.add("Your wounds start to feel better!", LIGHT_VIOLET);
        player.heal(HEAL_AMOUNT, game);
        return UseResult::UsedUp
    }
    UseResult::Cancelled
}

fn cast_lightning(
    _inventory_id: usize,
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
) -> UseResult {
    // find closest enemy in max range and dmage it
    let monster_id = closest_monster(tcod, objects, LIGHTNING_RANGE);
    if let Some(monster_id) = monster_id {
        // zap it!
        game.messages.add(
            format!("A lightning bolt strikes the {} with a loud thunderclap! \
            It takes {} damage!", objects[monster_id].name, LIGHTNING_DAMAGE), LIGHT_BLUE,
        );
        objects[monster_id].take_damage(LIGHTNING_DAMAGE, game);
        UseResult::UsedUp
    } else {
        // no enemy found in max range
        game.messages.add("No enemy is close enough to strike.", RED);
        UseResult::Cancelled
    }
}

fn cast_confuse(
    _inventory_id: usize,
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
) -> UseResult {
    // ask a player for enemy in-range and confuse it
    game.messages.add(
        "Left-click an enemy to confuse it, or right-click to cancel.",
        LIGHT_CYAN,
    );


    let monster_id = target_monster(tcod, game, objects, Some(CONFUSE_RANGE as f32));
    if let Some(monster_id) = monster_id {
        let old_ai = objects[monster_id].ai.take().unwrap_or(AI::Basic);
        // replace the monster's AI with a confused one
        // after some turns it will restore to the old AI
        objects[monster_id].ai = Some(AI::Confused {
            previous_ai: Box::new(old_ai),
            num_turns: CONFUSE_NUM_TURNS,
        });
        game.messages.add(
            format!(
                "The eyes of {} look vacant as it starts to stumble around!",
                objects[monster_id].name
        ),
        LIGHT_GREEN,
    );
    UseResult::UsedUp
    } else {
        // no enemy found within max range
        game.messages.add("No enemy is close enough to strike", RED);
        UseResult::Cancelled
    }
}

fn cast_fireball(
    _inventory_id: usize,
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
) -> UseResult {
    // ask the player for a target tile to throw fireball at
    game.messages.add(
        "Left-click a target tile for the fireball or right-click to cancel.",
        LIGHT_AMBER,
    );
    let (x, y) = match target_tile(tcod, game, objects, None) {
        Some(tile_pos) => tile_pos,
        None => return UseResult::Cancelled,
    };
    game.messages.add(
        format!(
            "The fireball explodes, burning everything within {} tiles!",
            FIREBALL_RADIUS
        ),
        ORANGE,
    );

    let mut xp_to_gain = 0;

    for (id, object) in objects.iter_mut().enumerate() {
        if object.distance(x, y) <= FIREBALL_RADIUS as f32 && object.fighter.is_some() {
            game.messages.add(
                format!(
                    "The {} gets burned for {} hit points.",
                    object.name, FIREBALL_DAMAGE
                ),
                ORANGE,
            );
            if let Some(xp) = object.take_damage(FIREBALL_DAMAGE, game) {
                if id != PLAYER {
                    // don't reward player for burning themselves
                    xp_to_gain += xp;
                }
            };
        }
    }
    objects[PLAYER].fighter.as_mut().unwrap().xp += xp_to_gain;
    UseResult::UsedUp
}

fn player_ranged_attack(
    inventory_id: usize,
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
) -> UseResult {
    // ask a player for enemy in-range and confuse it
    game.messages.add(
        "Left-click an enemy to shoot it, or right-click to cancel.",
        LIGHT_CYAN,
    );

    let equipment = &mut game.inventory[inventory_id].equipment.unwrap();

    let monster_id = target_monster(tcod, game, objects, Some(equipment.range as f32));
    
    if let Some(monster_id) = monster_id {

        let (player, target) = mut_two(PLAYER, monster_id, objects);

        let damage = equipment.damage - target.defense(game);

        if damage > 0 {
            // make the target take damage
            game.messages.add(
                format!(
                    "Your projectile strikes {} for {} hit points.",
                    target.name, equipment.damage
                ),
                ORANGE,
            );
            if let Some(xp) = target.take_damage(damage, game) {
                // yield experience to player if target killed
                player.fighter.as_mut().unwrap().xp += xp;
            };
        } else {
            game.messages.add(format!(
                "{}'s projectile strikes {}, but it has no affect!",
                player.name, target.name
            ), GREEN);
        };
        // if charges are below 0, keep, else used up
        if equipment.charges == 1 {
            game.messages.add(
                "You are out of ammo!",
                ORANGE,
            );
            UseResult::UsedUp
        } else {
            UseResult::UseCharge
        }
    } else {
        // no enemy found within max range
        game.messages.add("No enemy is close enough to strike", RED);
        UseResult::Cancelled
    }
}

/// Find closest enemy, up to a max range and in FOV
fn closest_monster(tcod: &mut Tcod, objects: &mut [Object], max_range: i32) -> Option<usize> {
    let mut closest_enemy = None;
    let mut closest_dist = (max_range + 1) as f32;

    for (id, object) in objects.iter().enumerate() {
        if (id != PLAYER)
            && object.fighter.is_some()
            && object.ai.is_some()
            && tcod.fov.is_in_fov(object.x, object.y)
        {
            // calculate distance between this object and the player
            let dist = objects[PLAYER].distance_to(object);
            if dist < closest_dist {
                // it's closer, so remember it
                closest_enemy = Some(id);
                closest_dist = dist;
            }
        }
    }
    closest_enemy
}

// basic AI functionality
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
enum AI {
    Basic,
    Ranged {
        range: f32,
    },
    Confused {
        previous_ai: Box<AI>,
        num_turns: i32,
    },
}

fn ai_take_turn(monster_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) {
    use AI::*;

    if let Some(ai) = objects[monster_id].ai.take() {
        let new_ai = match ai {
            Basic => ai_basic(monster_id, tcod, game, objects),
            Ranged { range } => ai_ranged(monster_id, tcod, game, objects, range),
            Confused {
                previous_ai,
                num_turns,
            } => ai_confused(monster_id, tcod, game, objects, previous_ai, num_turns),
        };
        objects[monster_id].ai = Some(new_ai);
    }
}

fn ai_basic(monster_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) -> AI {
    // a basic monster takes its turn. If you can see it, it can see you
    let (monster_x, monstery_y) = objects[monster_id].pos();
    if tcod.fov.is_in_fov(monster_x, monstery_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            // move towards player if far away
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, &game.map, objects);
        } else {
            // close enough, attack! (if the player is still alive)
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.attack(player, game);
        }
    }
    AI::Basic
}

fn ai_ranged(monster_id: usize, tcod: &mut Tcod, game: &mut Game, objects: &mut [Object], range: f32) -> AI {
    // a basic monster takes its turn. If you can see it, it can see you
    let (monster_x, monstery_y) = objects[monster_id].pos();
    if tcod.fov.is_in_fov(monster_x, monstery_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= range {
            // move towards player if far away
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, &game.map, objects);
        } else {
            // close enough, attack! (if the player is still alive)
            let (monster, player) = mut_two(monster_id, PLAYER, objects);
            monster.ranged_attack(player, game, range);
        }
    }
    AI::Ranged { range }
}

fn ai_confused(
    monster_id: usize, 
    _tcod: &mut Tcod,
    game: &mut Game, 
    objects: &mut [Object],
    previous_ai: Box<AI>,
    num_turns: i32,
) -> AI {
    if num_turns > 0 {
        // still confused
        // move in a random direction, then decreaes number of turns confused
        move_by(
            monster_id,
            rand::thread_rng().gen_range(-1, 2),
            rand::thread_rng().gen_range(-1, 2),
            &game.map,
            objects,
        );
        AI::Confused {
            previous_ai: previous_ai,
            num_turns: num_turns - 1,
        }
    } else {
        // restore previous AI
        game.messages.add(
            format!("The {} is no longer confused!", objects[monster_id].name),
            RED,
        );
        *previous_ai
    }
}

/// Mutably borrow two *separate* elements from a given slice.
/// Panics when the indexes are equal or out of bounds.
fn mut_two<T>(first_index: usize, second_index: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_index != second_index);
    let split_at_index = cmp::max(first_index, second_index);
    let (first_slice, second_slice) = items.split_at_mut(split_at_index);
    if first_index < second_index {
        (&mut first_slice[first_index], &mut second_slice[0])
    } else {
        (&mut second_slice[0], &mut first_slice[second_index])
    }
}

fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let (x, y) = objects[id].pos();
    if !is_blocked(x + dx, y + dy, map, objects) {
        objects[id].set_pos(x + dx, y + dy);
    }
}

fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, objects: &mut [Object], tcod: &mut Tcod) {
    // the coordinates the player is moving to/attacking
    let x = objects[PLAYER].x + dx;
    let y = objects[PLAYER].y + dy;

    let footstep = rodio::Decoder::new(BufReader::new(File::open("footstep03.ogg").unwrap())).unwrap();
    let cut = rodio::Decoder::new(BufReader::new(File::open("knifeSlice.ogg").unwrap())).unwrap();

    // try to find an attackable object there
    let target_id = objects
        .iter()
        .position(|object| object.fighter.is_some() && object.pos() == (x, y));

    // attack if target found, move otherwise
    match target_id {
        Some(target_id) => {
            let (player, target) = mut_two(PLAYER, target_id, objects);
            player.attack(target, game);
            tcod.sink.append(cut);
        }
        None => {
            move_by(PLAYER, dx, dy, &game.map, objects);
            tcod.sink.append(footstep);
        }
    }
}

fn move_towards(id: usize, target_x: i32, target_y: i32, map: &Map, objects: &mut [Object]) {
    // vector from this object to the target and distance
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx.pow(2) + dy.pow(2)) as f32).sqrt();

    // normalize to length 1 (preserving direction), then round it and
    // convert to integer so the movement is restricted to map grig
    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, map, objects);
}

/// Advance to the next level
fn next_level(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) {
    game.messages.add(
        "You take a moment to rest and recover your strength.",
        VIOLET,
    );
    let heal_hp = objects[PLAYER].max_hp(game) / 2;
    objects[PLAYER].heal(heal_hp, game);

    game.messages.add(
        "After a rare moment of peace, you descend deeper into \
        the heart of the dungeon...",
        RED,
    );

    game.dungeon_level += 1;
    game.map = make_map(objects, game.dungeon_level);
    initialize_fov(tcod, &game.map);
}

struct Transition {
    level: u32,
    value: u32,
}

/// Returns a value that depends on leve. The table specifies what
/// value occurs after each level, default is 0
fn from_dungeon_level(table: &[Transition], level: u32) -> u32 {
    table
        .iter()
        .rev()
        .find(|transition| level >= transition.level)
        .map_or(0, |transition| transition.value)
}

fn place_objects(room: Rect, map: &Map, objects: &mut Vec<Object>, level: u32) {

    let max_monsters = from_dungeon_level(
        &[
            Transition { level: 1, value: 2 },
            Transition { level: 4, value: 3 },
            Transition { level: 6, value: 5 },
        ],
        level,
    );


    let num_monsters = rand::thread_rng().gen_range(0, max_monsters + 1);

    let troll_chance = from_dungeon_level( 
        &[
            Transition {
                level: 3,
                value: 15,
            },
            Transition {
                level: 5,
                value: 30,
            },
            Transition {
                level: 7,
                value: 60,
            },
        ],
        level,
        );

    let shaman_chance = from_dungeon_level( 
        &[
            Transition {
                level: 2,
                value: 15,
            },
            Transition {
                level: 4,
                value: 30,
            },
            Transition {
                level: 5,
                value: 30,
            },
        ],
        level,
        );
    

    let scorpion_chance = from_dungeon_level( 
        &[
            Transition {
                level: 5,
                value: 15,
            },
            Transition {
                level: 7,
                value: 30,
            },
            Transition {
                level: 9,
                value: 60,
            },
        ],
        level,
        );

        for _ in 0..num_monsters {
            let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
            let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

            if !is_blocked(x, y, map, objects) {
            // Randomly select monster
            let monster_chances = [
                ("broo", 80),
                ("troll", troll_chance),
                ("broo shaman", shaman_chance),
                ("scorpion man", scorpion_chance),
            ];

            let dist = WeightedIndex::new(monster_chances.iter().map(
                |item| item.1)).unwrap();    
                
            let mut rng = rand::thread_rng();

            let choice = monster_chances[dist.sample(&mut rng)].0;

            let mut monster = match choice {
                "broo" => Object {
                    x: x,
                    y: y,
                    glyph: 'b',
                    color: DESATURATED_CRIMSON,
                    name: "Broo".to_string(),
                    blocks: true,
                    alive: true,
                    fighter: Some(Fighter {
                        base_max_hp: 20,
                        hp: 20,
                        base_defense: 0,
                        base_power: 4,
                        xp: 35,
                        on_death: DeathCallback::Monster,
                    }),
                    ai: Some(AI::Basic),
                    item: None,
                    always_visible: false,
                    level: 1,
                    equipment: None,
                },
                "broo shaman" => Object {
                    x: x,
                    y: y,
                    glyph: 's',
                    color: DESATURATED_CRIMSON,
                    name: "Broo Shaman".to_string(),
                    blocks: true,
                    alive: true,
                    fighter: Some(Fighter {
                        base_max_hp: 20,
                        hp: 20,
                        base_defense: 0,
                        base_power: 4,
                        xp: 60,
                        on_death: DeathCallback::Monster,
                    }),
                    ai: Some(AI::Ranged { range: 4.0 }),
                    item: None,
                    always_visible: false,
                    level: 3,
                    equipment: None,
                },
                "troll" => Object {
                        x: x,
                        y: y,
                        glyph: 'T',
                        color: DARK_GREEN,
                        name: "Troll".to_string(),
                        blocks: true,
                        alive: true,
                        fighter: Some(Fighter {
                            base_max_hp: 30,
                            hp: 30,
                            base_defense: 2,
                            base_power: 8,
                            xp: 100,
                            on_death: DeathCallback::Monster,
                        }),
                        ai: Some(AI::Basic),
                        item: None,
                        always_visible: false,
                        level: 3,
                        equipment: None,
                    },
                "scorpion man" => Object {
                    x: x,
                    y: y,
                    glyph: 'S',
                    color: BRASS,
                    name: "Scorpion Man".to_string(),
                    blocks: true,
                    alive: true,
                    fighter: Some(Fighter {
                        base_max_hp: 40,
                        hp: 40,
                        base_defense: 2,
                        base_power: 10,
                        xp: 125,
                        on_death: DeathCallback::Monster,
                    }),
                    ai: Some(AI::Basic),
                    item: None,
                    always_visible: false,
                    level: 4,
                    equipment: None,
                },
            _ => unreachable!(),
            };
    
            objects.push(monster);
        }
    }

    // Place Items

    let max_items = from_dungeon_level(
        &[
            Transition { level: 1, value: 1 },
            Transition { level: 4, value: 2 },
        ],
        level,
    );

    // Item random table
    let item_chances = [
        (Item::Heal, 35),
        (Item::Lightning, from_dungeon_level(
            &[
                Transition {
                    level: 4,
                    value: 25,
                }
            ],
            level,
        )),
        (Item::Fireball, from_dungeon_level(
            &[
                Transition {
                    level: 6,
                    value: 25,
                }
            ],
            level,
        )),
        (Item::Confuse, from_dungeon_level(
            &[
                Transition {
                    level: 2,
                    value: 10,
                }
            ],
            level,
        )),
        (Item::Sword, from_dungeon_level(
            &[
                Transition {
                    level: 1,
                    value: 10,
                }
            ],
            level,
        )),
        (Item::Shield, from_dungeon_level(
            &[
                Transition {
                    level: 1,
                    value: 10,
                }
            ],
            level,
        )),
        (Item::Helmet, from_dungeon_level(
            &[
                Transition {
                    level: 1,
                    value: 10,
                }
            ],
            level,
        )),
        (Item::Bow, from_dungeon_level(
            &[
                Transition {
                    level: 1,
                    value: 100,
                }
            ],
            level,
        )),
    ];
    
    let item_dist = WeightedIndex::new(item_chances.iter().map(
        |item| item.1)).unwrap();    
      

    let num_items = rand::thread_rng().gen_range(0, max_items + 1);

    for _ in 0..num_items {
        // choose random spot for this item
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if !is_blocked(x, y, map, objects) {
  
            let mut rng = rand::thread_rng();

            let choice = item_chances[item_dist.sample(&mut rng)].0;

            let mut item = match choice {
                Item::Heal => {
                // create healing potion
                let mut object = Object::new(x, y, '!', VIOLET, "healing potion".to_string(), false);
                object.item = Some(Item::Heal);
                object
            }
            Item::Lightning => {
                // create a lightning bolt scroll
                let mut object = Object::new(
                    x,
                    y, 
                    '#',
                    LIGHT_YELLOW,
                    "scroll of lightning bolt".to_string(),
                    false,
                );
                object.item = Some(Item::Lightning);
                object
            } 
            Item::Confuse => {
                // create confuse scroll (10% chance)
                let mut object = Object::new(
                    x,
                    y,
                    '?',
                    LIGHT_AZURE,
                    "scroll of confusion".to_string(),
                    false,
                );
                object.item = Some(Item::Confuse);
                object
            } 
            Item::Fireball => {
                // create fireball scroll
                let mut object = Object::new(
                    x,
                    y,
                    'F',
                    ORANGE,
                    "scroll of fireball".to_string(),
                    false,
                );
                object.item = Some(Item::Fireball);
                object
            }
            Item::Sword => {
                // create a sword
                let mut object = Object::new(x, y, '/', SKY, "sword".to_string(), false);
                object.item = Some(Item::Sword);

                match level {
                    1 | 2 => {
                        object.name = "short sword".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::RightHand, power_bonus: 3, defense_bonus: 0, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    3 | 4 | 5 => {
                        object.name = "broadsword".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::RightHand, power_bonus: 4, defense_bonus: 0, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    6 | 7 | 8 => {
                        object.name = "fine sword".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::RightHand, power_bonus: 6, defense_bonus: 0, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    l if l > 8 => {
                        object.name = "enchanted sword".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::RightHand, power_bonus: 8, defense_bonus: 0, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    _ => unreachable!()
                }
                object
            }
            Item::Shield => {
                // create a shield
                let mut object = Object::new(x, y, ')', SKY, "shield".to_string(), false);
                object.item = Some(Item::Shield);

                match level {
                    1 | 2 => {
                        object.name = "wooden shield".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::LeftHand, power_bonus: 0, defense_bonus: 2, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    3 | 4 | 5 => {
                        object.name = "round shield".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::LeftHand, power_bonus: 0, defense_bonus: 3, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    6 | 7 | 8 => {
                        object.name = "kite shield".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::LeftHand, power_bonus: 0, defense_bonus: 4, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    l if l > 8 => {
                        object.name = "enchanted shield".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::LeftHand, power_bonus: 0, defense_bonus: 6, max_hp_bonus: 0, range: 0, damage: 0, charges: 0});
                    }
                    _ => unreachable!()
                }
                object
            }
            Item::Helmet => {
                // create a helmet
                let mut object = Object::new(x, y, 'M', SKY, "helmet".to_string(), false);
                object.item = Some(Item::Helmet);

                match level {
                    1 | 2 => {
                        object.name = "leather helmet".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Head, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 15, range: 0, damage: 0, charges: 0});
                    }
                    3 | 4 | 5 => {
                        object.name = "pot helm".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Head, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 30, range: 0, damage: 0, charges: 0});
                    }
                    6 | 7 | 8 => {
                        object.name = "full helm".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Head, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 45, range: 0, damage: 0, charges: 0});
                    }
                    l if l > 8 => {
                        object.name = "enchanted helm".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Head, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 80, range: 0, damage: 0, charges: 0});
                    }
                    _ => unreachable!()
                }
                object
            }
            Item::Bow => {
                // create a helmet
                let mut object = Object::new(x, y, '}', SKY, "bow".to_string(), false);
                object.item = Some(Item::Bow);

                match level {
                    1 | 2 => {
                        object.name = "short bow".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Back, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 0, range: 4, damage: 5, charges: 12});
                    }
                    3 | 4 | 5 => {
                        object.name = "longbow".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Back, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 0, range: 5, damage: 6, charges: 12});
                    }
                    6 | 7 | 8 => {
                        object.name = "crossbow".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Back, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 0, range: 6, damage: 8, charges: 12});
                    }
                    l if l > 8 => {
                        object.name = "magic bow".to_string();
                        object.equipment = Some(Equipment{ equipped: false, slot: Slot::Back, power_bonus: 0, defense_bonus: 0, max_hp_bonus: 0, range: 8, damage: 10, charges: 12});
                    }
                    _ => unreachable!()
                }
                object
            }
        };
        item.always_visible = true;
        objects.push(item);
        }
    }
}

fn render_all(tcod: &mut Tcod, game: &mut Game, objects: &[Object], fov_recompute: bool) {
    // draw all objects in the list

    if fov_recompute {
        // recompute FOV
        let player = &objects[0];
        tcod.fov
            .compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);
    }

    let mut to_draw: Vec<_> = objects
        .iter()
        .filter(|o| {
            tcod.fov.is_in_fov(o.x, o.y)
            || (o.always_visible && game.map[o.x as usize][o.y as usize].explored) 
        })
        .collect();
    // sort so that non-blocking objects come first
    to_draw.sort_by(|o1, o2| { o1.blocks.cmp(&o2.blocks) });
    // draw the objects in the list
    for object in &to_draw {
        object.draw(&mut tcod.con);
    }

    // Go through all tiles and set their background color
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let visible = tcod.fov.is_in_fov(x, y);
            let wall = game.map[x as usize][y as usize].block_sight;
            let color = match (visible, wall) {
                // outside of FOV
                (false, true) => COLOR_DARK_WALL,
                (false, false) => COLOR_DARK_GROUND,
                // inside FOV
                (true, true) => COLOR_LIGHT_WALL,
                (true, false) => COLOR_LIGHT_GROUND,
            };

            let explored = &mut game.map[x as usize][y as usize].explored;
            if visible {
                // since it's visible, explore it
                *explored = true;
            }
            if *explored {
                // show explored tiels only (any visible tile is explored already)
                tcod.con
                    .set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }    
    }

    // prepare to render panel
    tcod.panel.set_default_background(BLACK);
    tcod.panel.clear();
    
    // show the player's stats
    let hp = objects[PLAYER].fighter.map_or(0, |f| f.hp);
    let max_hp = objects[PLAYER].max_hp(game);

    render_bar(&mut tcod.panel,
        1, 
        1, 
        BAR_WIDTH,
        "HP",
        hp,
        max_hp,
        LIGHT_RED,
        DARKER_RED,
    );

    // render dungeon level
    tcod.panel.print_ex(
        1,
        3,
        BackgroundFlag::None,
        TextAlignment::Left,
        format!("Dungeon level: {}", game.dungeon_level),
    );

    // display names under the mouse
    tcod.panel.set_default_foreground(LIGHT_GREY);
    tcod.panel.print_ex(
        1,
        0,
        BackgroundFlag::None,
        TextAlignment::Left,
        get_names_under_mouse(tcod.mouse, objects, &tcod.fov),
    );

    // print game messages, one line at a time
    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    blit(
        &tcod.panel,
        (0, 0),
        (SCREEN_WIDTH, PANEL_HEIGHT),
        &mut tcod.root,
        (0, PANEL_Y),
        1.0,
        1.0,
    );

    blit(
        &tcod.con,
        (0, 0),
        (MAP_WIDTH, MAP_HEIGHT),
        &mut tcod.root,
        (0, 0),
        1.0,
        1.0,
    );
}

fn handle_keys(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) -> PlayerAction {
    use PlayerAction::*;

    let player_alive = objects[PLAYER].alive;

    match (tcod.key, tcod.key.text(), player_alive) {
        // Alt+Enter: toggle fullscreen
        (
        Key {
            code: Enter,
            alt: true,
            ..
        },
        _,
        _,
        ) => {
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            DidntTaketurn
        },
        (Key { code: Escape, ..}, _, _ )=> return Exit, // exit game

        // rest
        // movement keys
        (Key { code: Spacebar, ..}, _, true) => {
            game.messages.add(
                format!("{} waits.", objects[PLAYER].name), BLUE);
            TookTurn
        },

        // movement keys
        (Key { code: Up, ..}, _, true) => {
            player_move_or_attack(0, -1, game, objects, tcod);
            TookTurn
        },
        (Key { code: Down, ..}, _, true) => {
            player_move_or_attack(0, 1, game, objects, tcod);
            TookTurn
        },
        (Key { code: Left, ..}, _, true) => {
            player_move_or_attack(-1, 0, game, objects, tcod);
            TookTurn
        },
        (Key { code: Right, ..}, _, true) => {
            player_move_or_attack(1, 0, game, objects, tcod);
            TookTurn
        },

        (Key { code: Text, ..}, "i", true) => {
            // show the inventory
            let inventory_index = inventory_menu(
                &game.inventory,
                "Press the key next to an item to use it, or any other key to cancel.\n",
                &mut tcod.root,
            );
            if let Some(inventory_index) = inventory_index {
                use_item(inventory_index, tcod, game, objects);
            }
            TookTurn
        },

        (Key { code: Text, ..}, "d", true) => {
            // show the inventory; if an item is selected, drop it
            let inventory_index = inventory_menu(
                &game.inventory,
                "Press the key next to an item to drop it, or any other key to cancel.\n",
                &mut tcod.root,
            );
            if let Some(inventory_index) = inventory_index {
                drop_item(inventory_index, game, objects);
            }
            DidntTaketurn
        },

        (Key { code: Text, .. }, "c", true) => {
            // show character information
            let player = &objects[PLAYER];
            let level = player.level;
            let level_up_xp = LEVEL_UP_BASE + player.level * LEVEL_UP_FACTOR;
            if let Some(fighter) = player.fighter.as_ref() {
                let msg = format!(
                    "Character Information
                    
Level: {}
Experience: {}
Experience to level up: {}

Maximum HP: {}
Attack: {}
Defense: {}",
                    level, fighter.xp, level_up_xp, player.max_hp(game), player.power(game), player.defense(game)
                );
                msgbox(&msg, CHARACTER_SCREEN_WIDTH, &mut tcod.root);
            }
            DidntTaketurn
        }

        (Key { code: Text, ..}, "g", true) => {
            // pick up an item
            let item_id = objects
                .iter()
                .position(|object| object.pos() == objects[PLAYER].pos() && object.item.is_some());
            if let Some(item_id) = item_id {
                pick_item_up(item_id, game, objects);
            }
            TookTurn
        },

        (Key { code: Text, ..}, "<", true) => {
            // go down stairs if the player is on them
            let player_on_stairs = objects
                .iter()
                .any(|object| object.pos() == objects[PLAYER].pos() && object.name == "stairs");
            if player_on_stairs {
                next_level(tcod, game, objects);
            }
            DidntTaketurn
        }
        _ => DidntTaketurn
    }
}

// Inventory
fn menu<T: AsRef<str>>(header: &str, options: &[T], width: i32, root: &mut Root) -> Option<usize> {

    assert!(
        options.len() <= 26,
        "Cannot have a menu with more than 26 options."
    );

    // calculate total height for the header (after auto-wrap) and one line per option
    let header_height = if header.is_empty() {
        0
    } else {
        root.get_height_rect(0, 0, width, SCREEN_HEIGHT, header)
    };
    let height = options.len() as i32 + header_height;

    let mut window = Offscreen::new(width, height);

    window.set_default_foreground(WHITE);
    window.print_rect_ex(
        0,
        0,
        width,
        height,
        BackgroundFlag::None,
        TextAlignment::Left,
        header,
    );

    // print all the options
    for (index, option_text) in options.iter().enumerate() {
        let menu_letter = (b'a' + index as u8) as char;
        let text = format!("({}) {}", menu_letter, option_text.as_ref());
        window.print_ex(
            0,
            header_height + index as i32,
            BackgroundFlag::None,
            TextAlignment::Left,
            text,
        );
    }

    // blit contents of "window" to root console

    let x = SCREEN_WIDTH / 2 - width / 2;
    let y = SCREEN_HEIGHT / 2 - height / 2;
    blit(&window, (0, 0), (width, height), root, (x, y), 1.0, 0.7);

    // present the root console to the player and wait for a key-press
    root.flush();
    let key = root.wait_for_keypress(true);

    // convert ASCII code to an index
    if key.printable.is_alphabetic() {
        let index = key.printable.to_ascii_lowercase() as usize - 'a' as usize;
        if index < options.len() {
            Some(index)
        } else {
            None
        }
    } else {
        None
    }
}

fn inventory_menu(inventory: &[Object], header: &str, root: &mut Root) -> Option<usize> {
    // how a menu with each item as an option
    let options = if inventory.len() == 0 {
        vec!["Inventory is empty".into()]
    } else {
        inventory
            .iter()
            .map(|item| {
                // show additional information in case it's equippped
                if item.equipment.is_some() {
                    let equip = item.equipment.unwrap();
                    let name = match equip {
                        e if e.power_bonus > 0 => format!("{} +{}pow", item.name, e.power_bonus),
                        e if e.defense_bonus > 0 => format!("{} +{}def", item.name, e.defense_bonus),
                        e if e.max_hp_bonus > 0 => format!("{} +{}hp", item.name, e.max_hp_bonus),
                        e if e.charges > 0 => format!("{} {} dam, {} range, {} charges", item.name, e.damage, e.range, e.charges),
                        _ => format!("{}", item.name)
                    };
                    if equip.equipped {
                        format!("{} (on {})", name, equip.slot)
                    } else {
                        format!("{}", name)
                    }
                } else {
                    item.name.clone()
                }
            })
            .collect()
    };

    let inventory_index = menu(header, &options, INVENTORY_WIDTH, root);

    // if an item was chosen, return it
    if inventory.len() > 0 {
        inventory_index
    } else {
        None
    }
}

/// return the position of a tile lef-clicked in player's FOV
/// optionally in range or (None, None) if right-clicked
fn target_tile(
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
    max_range: Option<f32>,
) -> Option<(i32, i32)> {
    loop {
        // render the screen, this erases the inventory and shows the names
        // of objects under the mouse
        tcod.root.flush();
        let event = input::check_for_event(input::KEY_PRESS | input::MOUSE).map(|e| e.1);
        match event {
            Some(Event::Mouse(m)) => tcod.mouse = m,
            Some(Event::Key(k)) => tcod.key = k,
            None => tcod.key = Default::default(),
        }
        render_all(tcod, game, objects, false);

        let (x, y) = (tcod.mouse.cx as i32, tcod.mouse.cy as i32);

        // accept the target if the player clicked in FOV, and in case a range
        // is specified, if it's in that range
        let in_fov = (x < MAP_WIDTH) && (y < MAP_HEIGHT) && tcod.fov.is_in_fov(x, y);
        let in_range = max_range.map_or(true, |range| objects[PLAYER].distance(x, y) <= range);
        if tcod.mouse.lbutton_pressed && in_fov && in_range {
            return Some((x, y))
        }
        if tcod.mouse.rbutton_pressed || tcod.key.code == Escape {
            return None; // cancel if player right-clicked or hit escape
        }
    }
}

/// returns a clicked monster inside FOV up to a range, or None if right-clicked
fn target_monster(
    tcod: &mut Tcod,
    game: &mut Game,
    objects: &mut [Object],
    max_range: Option<f32>,
) -> Option<usize> {
    loop {
        match target_tile(tcod, game, objects, max_range) {
            Some((x, y)) => {
                // return the first clicked monster, otherwise continue looping
                for (id, obj) in objects.iter().enumerate() {
                    if obj.pos() == (x, y) && obj.fighter.is_some() && id != PLAYER {
                        return Some(id);
                    }
                }
            }
            None => return None,
        }
    }
}

// return a string with the names of all objects under the mouse
fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    // create a list of names of all objects at the mouse's coordinates in FOV
    let names = objects
        .iter()
        .filter(|obj| obj.pos() == (x, y) && fov_map.is_in_fov(obj.x, obj.y))
        .map(|obj| obj.name.clone())
        .collect::<Vec<_>>();

    names.join(", ") // join the names
}

// GUI Rendering
fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: Color,
    back_color: Color,
) {
    // render a bar (HP, XP, etc) -- first calculate width
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    // render background
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    // now render bar
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    // finally centered text with values
    panel.set_default_foreground(WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        &format!("{}: {}/{}", name, value, maximum),
    );
}

fn new_game(tcod: &mut Tcod) -> (Game, Vec<Object>) {
    // create object representing player

     // define player object
     let mut player = Object::new(0, 0, '@', WHITE, "player".to_string(), true);
     player.alive = true;
     player.fighter = Some(Fighter {
         base_max_hp: 100,
         hp: 100,
         base_defense: 1,
         base_power: 2,
         xp:  0,
         on_death: DeathCallback::Player,
     });
    
     // Vec of mutable objects
     let mut objects = vec![player];

    // generate map
    let mut game = Game {
        map: make_map(&mut objects, 1),
        messages: Messages::new(),
        inventory: vec![],
        dungeon_level: 1,
    };

    // initial equipment: a dagger
    let mut dagger = Object::new(0, 0, '-', SKY, "dagger".to_string(), false);
    dagger.item = Some(Item::Sword);
    dagger.equipment = Some(Equipment {
        equipped: true,
        slot: Slot::RightHand,
        max_hp_bonus: 0,
        defense_bonus: 0,
        power_bonus: 2,
        range: 0,
        damage: 0,
        charges: 0,
    });
    game.inventory.push(dagger);

    initialize_fov(tcod, &game.map);

    // Welcome message
    game.messages.add(
        "Welcome stranger! Prepare to perish in the Tombs of the Ancient Kings!",
        RED,
    );

    (game, objects)

}

fn initialize_fov(tcod: &mut Tcod, map: &Map) {
    // populate the FOV map, accorging to generated map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            tcod.fov.set(
                x,
                y,
                !map[x as usize][y as usize].block_sight,
                !map[x as usize][y as usize].blocked,
            );
        }
    }
    // unexplored areas start black
    tcod.con.clear();
}

fn play_game(tcod: &mut Tcod, game: &mut Game, objects: &mut Vec<Object>) {
    // force FOV "recompute" first time through the game loop
    let mut previous_player_position = (-1, -1);

    // Game Loop
    while !tcod.root.window_closed() {
        // clear contents of previous screen
        tcod.con.clear();

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => tcod.key = k,
            _ => tcod.key = Default::default(),
        }

        // render the screen
        let fov_recompute = previous_player_position != (objects[PLAYER].pos());
        render_all(tcod, game, &objects, fov_recompute);

        tcod.root.flush();

        // level up if needed
        level_up(tcod, game, objects);

        // handle keys and exit game if needed
        previous_player_position = objects[PLAYER].pos();
        let player_action = handle_keys(tcod, game, objects);
        if player_action == PlayerAction::Exit {
            save_game(game, objects).unwrap();
            break;
        }

        if objects[PLAYER].alive && player_action != PlayerAction::DidntTaketurn {
            for id in 0..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, tcod, game, objects);
                }
            }
        }
    }
}

/// msgbox
fn msgbox(text: &str, width: i32, root: &mut Root) {
    let options: &[&str] = &[];
    menu(text, options, width, root);
}

/// Menus
fn main_menu(tcod: &mut Tcod) {
    let img = tcod::image::Image::from_file("menu_background.png")
        .ok()
        .expect("Background image not found");

    while !tcod.root.window_closed() {
        // show background image at 2x regular resolution
        tcod::image::blit_2x(&img, (0, 0), (-1, -1), &mut tcod.root, (0, 0));

        tcod.root.set_default_foreground(LIGHT_YELLOW);
        tcod.root.print_ex(
            SCREEN_WIDTH / 2,
            SCREEN_HEIGHT / 2 - 4,
            BackgroundFlag::None,
            TextAlignment::Center,
            "Snakepipe Hollow",
        );

        tcod.root.print_ex(
            SCREEN_WIDTH / 2,
            SCREEN_HEIGHT / 2 + 4,
            BackgroundFlag::None,
            TextAlignment::Center,
            "By ToferC",
        );

        let device = rodio::default_output_device().unwrap();

        let title_sink = Sink::new(&device);   
        let title = rodio::Decoder::new(BufReader::new(File::open("AerisPianoByTannerHelland.ogg").unwrap())).unwrap();
        title_sink.append(title);

        // show options and wait for player's choice
        let choices = &["Play a new game", "Continue game", "Quit"];
        let choice = menu("", choices, 24, &mut tcod.root);

        match choice {
            Some(0) => {
                // New game
                title_sink.stop();
                let (mut game, mut objects) = new_game(tcod);
                play_game(tcod, &mut game, &mut objects);
            }
            Some(1) => {
                // load game
                match load_game() {
                    Ok((mut game, mut objects)) => {
                        title_sink.stop();
                        initialize_fov(tcod, &game.map);
                        play_game(tcod, &mut game, &mut objects);
                    }
                    Err(_e) => {
                        msgbox("\nNo saved game to load.\nHit Esc to return.\n", 24, &mut tcod.root);
                        continue;
                    }
                }
            }
            Some(2) => {
                // quit
                break;
            }
            _ => {}
        }
    }
}

fn save_game(game: &Game, objects: &[Object]) -> Result<(), Box<dyn Error>> {
    let save_data = serde_json::to_string(&(game, objects))?;
    let mut file = File::create("savegame")?;
    file.write_all(save_data.as_bytes())?;
    Ok(())
}

fn load_game() -> Result<(Game, Vec<Object>), Box<dyn Error>> {
    let mut json_save_state = String::new();
    let mut file = File::open("savegame")?;
    file.read_to_string(&mut json_save_state)?;
    let result = serde_json::from_str::<(Game, Vec<Object>)>(&json_save_state)?;
    Ok(result)
}

fn main() {

    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("Bisasam_16x16.png", FontLayout::AsciiInRow)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Snakepipe Hollow")
        .init();

    // configure audio
    let device = rodio::default_output_device().unwrap();
    let sink = Sink::new(&device);    

    let mut tcod = Tcod {
        root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        key: Default::default(),
        mouse: Default::default(),
        sink: sink,
     };

     main_menu(&mut tcod);

}
