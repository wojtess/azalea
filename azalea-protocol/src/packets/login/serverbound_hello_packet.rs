use azalea_buf::McBuf;
use azalea_protocol_macros::ServerboundLoginPacket;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, McBuf, ServerboundLoginPacket)]
pub struct ServerboundHelloPacket {
    pub name: String,
    pub chat_session: RemoteChatSessionData,
    pub profile_id: Option<Uuid>,
}

#[derive(Clone, Debug, PartialEq, Eq, McBuf)]
pub struct RemoteChatSessionData {
    pub session_id: Uuid,
    pub profile_public_key: Option<ProfilePublicKeyData>,
}

#[derive(Clone, Debug, McBuf, PartialEq, Eq)]
pub struct ProfilePublicKeyData {
    pub expires_at: u64,
    pub key: Vec<u8>,
    pub key_signature: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use azalea_buf::{McBufReadable, McBufWritable};

    #[test]
    fn test_read_write() {
        let packet = ServerboundHelloPacket {
            name: "test".to_string(),
            chat_session: RemoteChatSessionData {
                session_id: Uuid::default(),
                profile_public_key: None,
            },
            profile_id: Some(Uuid::from_u128(0)),
        };
        let mut buf: Vec<u8> = Vec::new();
        packet.write_into(&mut buf).unwrap();
        let packet2 = ServerboundHelloPacket::read_from(&mut Cursor::new(&buf)).unwrap();
        assert_eq!(packet, packet2);
    }
}
