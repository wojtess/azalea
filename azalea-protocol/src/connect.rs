//! Create connections that communicate with a remote server or client.

use crate::packets::game::{ClientboundGamePacket, ServerboundGamePacket};
use crate::packets::handshake::{ClientboundHandshakePacket, ServerboundHandshakePacket};
use crate::packets::login::clientbound_hello_packet::ClientboundHelloPacket;
use crate::packets::login::{ClientboundLoginPacket, ServerboundLoginPacket};
use crate::packets::status::{ClientboundStatusPacket, ServerboundStatusPacket};
use crate::packets::ProtocolPacket;
use crate::read::{read_packet, ReadPacketError};
use crate::write::write_packet;
use azalea_auth::sessionserver::SessionServerError;
use azalea_crypto::{Aes128CfbDec, Aes128CfbEnc};
use bytes::BytesMut;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use uuid::Uuid;

/// The read half of a connection.
pub struct ReadConnection<R: ProtocolPacket> {
    read_stream: OwnedReadHalf,
    buffer: BytesMut,
    compression_threshold: Option<u32>,
    dec_cipher: Option<Aes128CfbDec>,
    _reading: PhantomData<R>,
}

/// The write half of a connection.
pub struct WriteConnection<W: ProtocolPacket> {
    write_stream: OwnedWriteHalf,
    compression_threshold: Option<u32>,
    enc_cipher: Option<Aes128CfbEnc>,
    _writing: PhantomData<W>,
}

/// A connection that can read and write packets.
///
/// # Examples
///
/// Join an offline-mode server and go through the handshake.
/// ```rust,no_run
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let resolved_address = resolver::resolve_address(address).await?;
///     let mut conn = Connection::new(&resolved_address).await?;
///
///     // handshake
///     conn.write(
///         ClientIntentionPacket {
///         protocol_version: PROTOCOL_VERSION,
///         hostname: address.host.to_string(),
///         port: address.port,
///         intention: ConnectionProtocol::Login,
///     }
///     .get(),
/// )
/// .await?;
/// let mut conn = conn.login();
///
/// // login
/// conn.write(
///     ServerboundHelloPacket {
///         username,
///         public_key: None,
///         profile_id: None,
///     }
///     .get(),
/// )
/// .await?;
///
/// let (conn, game_profile) = loop {
///     let packet_result = conn.read().await;
///     match packet_result {
///         Ok(packet) => match packet {
///             ClientboundLoginPacket::Hello(p) => {
///                 let e = azalea_crypto::encrypt(&p.public_key, &p.nonce).unwrap();
///
///                 conn.write(
///                     ServerboundKeyPacket {
///                         nonce_or_salt_signature: NonceOrSaltSignature::Nonce(e.encrypted_nonce),
///                         key_bytes: e.encrypted_public_key,
///                     }
///                     .get(),
///                 )
///                 .await?;
///                 conn.set_encryption_key(e.secret_key);            }
///             ClientboundLoginPacket::LoginCompression(p) => {
///                 conn.set_compression_threshold(p.compression_threshold);
///             }
///             ClientboundLoginPacket::GameProfile(p) => {
///                 break (conn.game(), p.game_profile);
///             }
///             ClientboundLoginPacket::LoginDisconnect(p) => {
///                 println!("login disconnect: {}", p.reason);
///                 bail!(JoinError::Disconnected(p.reason));
///             }
///             ClientboundLoginPacket::CustomQuery(p) => {}
///         },
///         Err(e) => {
///             eprintln!("Error: {:?}", e);
///             bail!("Error: {:?}", e);
///         }
///     }
/// };
/// ```
pub struct Connection<R: ProtocolPacket, W: ProtocolPacket> {
    pub reader: ReadConnection<R>,
    pub writer: WriteConnection<W>,
}

impl<R> ReadConnection<R>
where
    R: ProtocolPacket + Debug,
{
    pub async fn read(&mut self) -> Result<R, ReadPacketError> {
        read_packet::<R, _>(
            &mut self.read_stream,
            &mut self.buffer,
            self.compression_threshold,
            &mut self.dec_cipher,
        )
        .await
    }
}
impl<W> WriteConnection<W>
where
    W: ProtocolPacket + Debug,
{
    /// Write a packet to the server.
    pub async fn write(&mut self, packet: W) -> std::io::Result<()> {
        write_packet(
            &packet,
            &mut self.write_stream,
            self.compression_threshold,
            &mut self.enc_cipher,
        )
        .await
    }

    /// End the connection.
    pub async fn shutdown(&mut self) -> std::io::Result<()> {
        self.write_stream.shutdown().await
    }
}

impl<R, W> Connection<R, W>
where
    R: ProtocolPacket + Debug,
    W: ProtocolPacket + Debug,
{
    /// Read a packet from the other side of the connection.
    pub async fn read(&mut self) -> Result<R, ReadPacketError> {
        self.reader.read().await
    }

    /// Write a packet to the other side of the connection.
    pub async fn write(&mut self, packet: W) -> std::io::Result<()> {
        self.writer.write(packet).await
    }

    /// Split the reader and writer into two objects. This doesn't allocate.
    pub fn into_split(self) -> (ReadConnection<R>, WriteConnection<W>) {
        (self.reader, self.writer)
    }
}

#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

impl Connection<ClientboundHandshakePacket, ServerboundHandshakePacket> {
    /// Create a new connection to the given address.
    pub async fn new(address: &SocketAddr) -> Result<Self, ConnectionError> {
        let stream = TcpStream::connect(address).await?;

        // enable tcp_nodelay
        stream.set_nodelay(true)?;

        let (read_stream, write_stream) = stream.into_split();

        Ok(Connection {
            reader: ReadConnection {
                read_stream,
                buffer: BytesMut::new(),
                compression_threshold: None,
                dec_cipher: None,
                _reading: PhantomData,
            },
            writer: WriteConnection {
                write_stream,
                compression_threshold: None,
                enc_cipher: None,
                _writing: PhantomData,
            },
        })
    }

    /// Change our state from handshake to login. This is the state that is used for logging in.
    pub fn login(self) -> Connection<ClientboundLoginPacket, ServerboundLoginPacket> {
        Connection::from(self)
    }

    /// Change our state from handshake to status. This is the state that is used for pinging the server.
    pub fn status(self) -> Connection<ClientboundStatusPacket, ServerboundStatusPacket> {
        Connection::from(self)
    }
}

impl Connection<ClientboundLoginPacket, ServerboundLoginPacket> {
    /// Set our compression threshold, i.e. the maximum size that a packet is
    /// allowed to be without getting compressed. If you set it to less than 0
    /// then compression gets disabled.
    pub fn set_compression_threshold(&mut self, threshold: i32) {
        // if you pass a threshold of less than 0, compression is disabled
        if threshold >= 0 {
            self.reader.compression_threshold = Some(threshold as u32);
            self.writer.compression_threshold = Some(threshold as u32);
        } else {
            self.reader.compression_threshold = None;
            self.writer.compression_threshold = None;
        }
    }

    /// Set the encryption key that is used to encrypt and decrypt packets. It's the same for both reading and writing.
    pub fn set_encryption_key(&mut self, key: [u8; 16]) {
        let (enc_cipher, dec_cipher) = azalea_crypto::create_cipher(&key);
        self.reader.dec_cipher = Some(dec_cipher);
        self.writer.enc_cipher = Some(enc_cipher);
    }

    /// Change our state from login to game. This is the state that's used when you're actually in the game.
    pub fn game(self) -> Connection<ClientboundGamePacket, ServerboundGamePacket> {
        Connection::from(self)
    }

    /// Authenticate with Minecraft's servers, which is required to join
    /// online-mode servers. This must happen when you get a
    /// `ClientboundLoginPacket::Hello` packet.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// let token = azalea_auth::auth(azalea_auth::AuthOpts {
    ///    ..Default::default()
    /// })
    /// .await;
    /// let player_data = azalea_auth::get_profile(token).await;
    ///
    /// let mut connection = azalea::Connection::new(&server_address).await?;
    ///
    /// // transition to the login state, in a real program we would have done a handshake first
    /// connection.login();
    ///
    /// match connection.read().await? {
    ///    ClientboundLoginPacket::Hello(p) => {
    ///       // tell Mojang we're joining the server
    ///       connection.authenticate(&token, player_data.uuid, p).await?;
    ///   }
    ///  _ => {}
    /// }
    /// ```
    pub async fn authenticate(
        &self,
        access_token: &str,
        uuid: &Uuid,
        private_key: [u8; 16],
        packet: ClientboundHelloPacket,
    ) -> Result<(), SessionServerError> {
        azalea_auth::sessionserver::join(
            access_token,
            &packet.public_key,
            &private_key,
            uuid,
            &packet.server_id,
        )
        .await
    }
}

// rust doesn't let us implement From because allegedly it conflicts with
// `core`'s "impl<T> From<T> for T" so we do this instead
impl<R1, W1> Connection<R1, W1>
where
    R1: ProtocolPacket + Debug,
    W1: ProtocolPacket + Debug,
{
    fn from<R2, W2>(connection: Connection<R1, W1>) -> Connection<R2, W2>
    where
        R2: ProtocolPacket + Debug,
        W2: ProtocolPacket + Debug,
    {
        Connection {
            reader: ReadConnection {
                read_stream: connection.reader.read_stream,
                buffer: connection.reader.buffer,
                compression_threshold: connection.reader.compression_threshold,
                dec_cipher: connection.reader.dec_cipher,
                _reading: PhantomData,
            },
            writer: WriteConnection {
                compression_threshold: connection.writer.compression_threshold,
                write_stream: connection.writer.write_stream,
                enc_cipher: connection.writer.enc_cipher,
                _writing: PhantomData,
            },
        }
    }
}
