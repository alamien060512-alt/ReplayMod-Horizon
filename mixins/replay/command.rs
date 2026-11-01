// /replay ...: runs on the server, for the player who typed it.
fn replay_mod_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    use crate::game::replay_mod as m;
    let mut words = command.split_whitespace();
    if !game.native_on(m::ID) || words.next() != Some("replay") {
        return;
    }
    *ran = true;
    let args: Vec<&str> = words.collect();
    let dir = game.native_data_dir(m::ID);
    let out = m::run_command(&dir, &player.dimension, &args);
    let dimension = player.dimension.clone();
    crate::game::replay_glue::apply(game, &dimension, &out.events);
    for line in out.say {
        game.native_feedback(horizon_client::text::Text::literal(&line));
    }
}
