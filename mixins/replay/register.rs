// The /replay usages join the server's command tree.
fn replay_mod_register(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    use crate::game::native_server::NativeCommand;
    use crate::game::replay_mod as m;
    for usage in m::USAGES {
        commands.push(NativeCommand::new(usage));
    }
}
