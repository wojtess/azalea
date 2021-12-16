use minecraft_protocol::packets::login::LoginPacket;

struct LoginListener {}

trait LoginListenerTrait {
    fn handle(packet: LoginPacket);
}

impl LoginListenerTrait for LoginListener {
    fn handle(packet: LoginPacket) {}
}
