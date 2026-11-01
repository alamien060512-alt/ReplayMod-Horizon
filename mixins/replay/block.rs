// A block changed on the server: the recorder keeps it with its old state.
fn replay_mod_block(
    dimension: &str,
    pos: horizon_core::position::BlockPos,
    old: horizon_world::registry::BlockStateId,
    new: horizon_world::registry::BlockStateId,
) {
    crate::game::replay_mod::record_block(dimension, [pos.x, pos.y, pos.z], old.0, new.0);
}
