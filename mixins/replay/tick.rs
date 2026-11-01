// After each level's tick: snapshot it while recording, step the playback
// while playing. Server thread only; the dimension names the level in place.
fn replay_mod_tick(game: &mut crate::game::Game<'_>, dimension: &str) {
    use crate::game::replay_mod as m;
    if !game.native_on(m::ID) {
        return;
    }
    if m::is_recording() {
        let mut now: Vec<m::Ent> = game
            .native_mobs(dimension)
            .into_iter()
            .map(|mob| m::Ent {
                key: format!("m{}", mob.id),
                kind: mob.kind,
                name: None,
                pos: mob.pos,
                yaw: mob.yaw,
            })
            .collect();
        now.extend(game.native_server_players().into_iter().filter(|p| p.dimension == dimension).map(|p| m::Ent {
            key: format!("p{:x}", p.uuid),
            kind: "player".into(),
            name: Some(p.name),
            pos: p.position,
            yaw: 0.0,
        }));
        m::record_tick(dimension, &now);
    } else if m::playing_dimension().as_deref() == Some(dimension) {
        let events = m::playback_tick();
        crate::game::replay_glue::apply(game, dimension, &events);
    }
}
