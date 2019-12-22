use tcod::colors::*;

use std::cmp;
use rand::Rng;

use crate::object::*;

const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

const MAX_ROOM_MONSTERS: i32 = 3;
const MAX_ROOM_ITEMS: i32 = 2;

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

// Tiles
#[derive(Clone, Copy, Debug)]
pub struct Tile {
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
pub struct Rect {
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
pub fn create_room( room: Rect, map: &mut Map) {
    // go through tiles in the rectangle and make them passable
    for x in (room.x1 + 1)..room.x2 {
        for y in (room.y1 + 1)..room.y2 {
            map[x as usize][y as usize] = Tile::empty();
        }
    }
}

pub fn create_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    // horizontal tunnel. min() and max() are used in case x1 > x2
    for x in cmp::min(x1, x2)..(cmp::max(x1, x2) + 1 ) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

pub fn create_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    // vertical tunnel as above
    for y in cmp::min(y1, y2)..(cmp::max(y1, y2) + 1) {
        map[x as usize][y as usize] = Tile::empty();
    }
}

// Map
pub type Map = Vec<Vec<Tile>>;

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

pub fn make_map(objects: &mut Vec<Object>) -> Map {

    // fill with blocked tiles
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];
    
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

            place_objects(new_room, &map, objects);

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
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                } else {
                    // first move horizontally, then vertically
                    create_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    create_h_tunnel(prev_x, new_x, prev_y, &mut map);
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

    map
}

fn place_objects(room: Rect, map: &Map, objects: &mut Vec<Object>) {
    let num_monsters = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

        for _ in 0..num_monsters {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

            if !is_blocked(x, y, map, objects) {
            // Randomly select monster
            let mut monster = if rand::random::<f32>() < 0.8 {
                Object {
                    x: x,
                    y: y,
                    glyph: 'o',
                    color: DESATURATED_GREEN,
                    name: "Orc".to_string(),
                    blocks: true,
                    alive: true,
                    fighter: Some(Fighter {
                        max_hp: 10,
                        hp: 10,
                        defense: 0,
                        power: 3,
                        on_death: DeathCallback::Monster,
                    }),
                    ai: Some(AI::Basic),
                    item: None,
                }
            } else {
                Object {
                    x: x,
                    y: y,
                    glyph: 'T',
                    color: DARK_GREEN,
                    name: "Troll".to_string(),
                    blocks: true,
                    alive: true,
                    fighter: Some(Fighter {
                        max_hp: 16,
                        hp: 16,
                        defense: 1,
                        power: 4,
                        on_death: DeathCallback::Monster,
                    }),
                    ai: Some(AI::Basic),
                    item: None,
                }
            };
    
            objects.push(monster);
        }
    }

    let num_items = rand::thread_rng().gen_range(0, MAX_ROOM_ITEMS + 1);

    for _ in 0..num_items {
        // choose random spot for this item
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

            if !is_blocked(x, y, map, objects) {
                let mut object = Object::new(x, y, '!', VIOLET, "healing potion".to_string(), false);
                object.item = Some(Item::Heal);
                objects.push(object);
            }
    }
}