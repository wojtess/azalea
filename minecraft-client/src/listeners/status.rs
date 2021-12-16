use minecraft_protocol::packets::status::StatusPacket;

struct StatusListener {}

impl StatusListenerTrait for StatusListener {
    fn handle(packet: StatusPacket) {}
}
