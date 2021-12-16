use minecraft_protocol::packets::handshake::HandshakePacket;

struct HandshakeListener {}

trait HandshakeListenerTrait {
    fn handle(packet: HandshakePacket);
}

impl HandshakeListenerTrait for HandshakeListener {
    fn handle(packet: HandshakePacket) {}
}
