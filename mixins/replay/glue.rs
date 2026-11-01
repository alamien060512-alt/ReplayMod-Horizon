// Carries the runtime's events out in the game: mobs spawned, moved and
// removed, blocks set. Players are stood in for by named villagers.
use super::Game;
use super::native_mobs::NativeSpawn;
use super::replay_mod as m;

/// Flag 2 (`UPDATE_CLIENTS`): no neighbour updates, so no drops or physics.
const FLAGS: i32 = 2;

pub fn apply(game: &mut Game<'_>, dimension: &str, events: &[m::Event]) {
    for event in events {
        match event {
            m::Event::Spawn(s) => {
                let kind = if s.kind == "player" { "minecraft:villager" } else { s.kind.as_str() };
                let options = NativeSpawn {
                    name: s.name.clone(),
                    no_ai: true,
                    invulnerable: true,
                    silent: true,
                    no_gravity: true,
                    ..Default::default()
                };
                if let Some(id) = game.native_spawn_mob(dimension, kind, s.pos, &options) {
                    m::bind(&s.key, id);
                }
            }
            m::Event::Move { key, pos, .. } => {
                if let Some(id) = m::live_id(key) {
                    game.native_teleport_mob(dimension, id, *pos);
                }
            }
            m::Event::Despawn(key) => {
                if let Some(id) = m::unbind(key) {
                    game.native_remove_mob(dimension, id);
                }
            }
            m::Event::Block { pos, new, .. } => {
                game.native_set_block(dimension, *pos, horizon_world::registry::BlockStateId(*new), FLAGS);
            }
        }
    }
}
