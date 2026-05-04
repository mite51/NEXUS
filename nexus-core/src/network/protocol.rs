//! Custom request-response protocol for shard exchange and kfrag delivery

use libp2p::request_response;
use serde::{Deserialize, Serialize};

/// NEXUS wire protocol messages
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NexusRequest {
    /// Request a shard by its CID (hex-encoded)
    GetShard { cid: String },
    /// Push a shard to a peer (sender-initiated transfer)
    PushShard { cid: String, data: Vec<u8> },
    /// Push a manifest + optional share grant to a peer
    PushManifest {
        manifest_json: String,
        share_grant_json: Option<String>,
    },
    /// Deliver kfrags to a recipient
    DeliverKfrags {
        manifest_id: String,
        kfrags: Vec<Vec<u8>>,
        verifying_key: Vec<u8>,
        sender_pre_pk: Vec<u8>,
    },
    /// Ping (health check)
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NexusResponse {
    /// Shard data
    Shard { cid: String, data: Vec<u8> },
    /// Shard not found
    ShardNotFound { cid: String },
    /// Acknowledge a pushed shard was stored
    ShardAccepted { cid: String },
    /// Acknowledge a pushed manifest was stored
    ManifestAccepted,
    /// kfrag delivery acknowledged
    KfragsReceived { manifest_id: String },
    /// Pong (health check response)
    Pong,
    /// Error
    Error { message: String },
}

/// Codec for serializing/deserializing protocol messages
#[derive(Debug, Clone, Default)]
pub struct NexusCodec;

#[derive(Debug, Clone)]
pub struct NexusProtocol;

impl AsRef<str> for NexusProtocol {
    fn as_ref(&self) -> &str {
        "/nexus/1.0.0"
    }
}

#[async_trait::async_trait]
impl request_response::Codec for NexusCodec {
    type Protocol = NexusProtocol;
    type Request = NexusRequest;
    type Response = NexusResponse;

    async fn read_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Request>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        read_length_prefixed_json(io).await
    }

    async fn read_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Response>
    where
        T: futures::AsyncRead + Unpin + Send,
    {
        read_length_prefixed_json(io).await
    }

    async fn write_request<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        req: Self::Request,
    ) -> std::io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        write_length_prefixed_json(io, &req).await
    }

    async fn write_response<T>(
        &mut self,
        _protocol: &Self::Protocol,
        io: &mut T,
        res: Self::Response,
    ) -> std::io::Result<()>
    where
        T: futures::AsyncWrite + Unpin + Send,
    {
        write_length_prefixed_json(io, &res).await
    }
}

const MAX_MSG_SIZE: usize = 16 * 1024 * 1024; // 16 MB (max shard + overhead)

async fn read_length_prefixed_json<T, D>(io: &mut T) -> std::io::Result<D>
where
    T: futures::AsyncRead + Unpin + Send,
    D: serde::de::DeserializeOwned,
{
    use futures::AsyncReadExt;

    let mut len_buf = [0u8; 4];
    io.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;

    if len > MAX_MSG_SIZE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("message too large: {} bytes", len),
        ));
    }

    let mut buf = vec![0u8; len];
    io.read_exact(&mut buf).await?;

    serde_json::from_slice(&buf).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("json decode: {}", e))
    })
}

async fn write_length_prefixed_json<T, S>(io: &mut T, value: &S) -> std::io::Result<()>
where
    T: futures::AsyncWrite + Unpin + Send,
    S: serde::Serialize,
{
    use futures::AsyncWriteExt;

    let data = serde_json::to_vec(value).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, format!("json encode: {}", e))
    })?;

    let len = (data.len() as u32).to_be_bytes();
    io.write_all(&len).await?;
    io.write_all(&data).await?;
    io.flush().await?;

    Ok(())
}
