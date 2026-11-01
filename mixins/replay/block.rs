// A block changed on the server: the recorder keeps it with its old state.
fn replay_mod_block(
    dimension: horizon_world::level::Dimension,
    pos: horizon_core::position::BlockPos,
    old: horizon_world::registry::BlockStateId,
    new: horizon_world::registry::BlockStateId,
) {
    use horizon_world::level::Dimension;
    let id = match dimension {
        Dimension::Overworld => "minecraft:overworld",
        Dimension::Nether => "minecraft:the_nether",
        Dimension::End => "minecraft:the_end",
    };
    crate::game::replay_mod::record_block(id, [pos.x, pos.y, pos.z], old.0, new.0);
}
