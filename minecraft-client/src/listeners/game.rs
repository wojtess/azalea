use minecraft_protocol::packets::game::GamePacket;

struct GameListener {}

trait GameListenerTrait {
    fn handle(packet: GamePacket);
}

impl GameListenerTrait for GameListener {
    fn handle(packet: GamePacket) {}
}
