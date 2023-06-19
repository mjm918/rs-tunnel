use anyhow::{Context, Result};
use std::time::Duration;
use serde::{Serialize, Deserialize};
use futures_util::{SinkExt, StreamExt};
use serde::de::DeserializeOwned;
use tokio::io;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;
use tokio_util::codec::{AnyDelimiterCodec, Framed, FramedParts};
use tracing::trace;
use uuid::Uuid;

pub const CONTROL_PORT: u16 = 7835;
pub const MAX_FRAME_LENGTH: usize = 256;
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Serialize, Deserialize)]
pub enum ClientMessage {
	Authenticate(String),
	Hello(u16),
	Accept(Uuid),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ServerMessage {
	Challenge(Uuid),
	Hello(u16),
	Heartbeat,
	Connection(Uuid),
	Error(String),
}

pub struct Delimited<U>(Framed<U, AnyDelimiterCodec>);

impl<U: AsyncRead + AsyncWrite + Unpin> Delimited<U> {
	pub fn new(stream: U) -> Self {
		let codec = AnyDelimiterCodec::new_with_max_length(vec![0], vec![0], MAX_FRAME_LENGTH);
		Self(Framed::new(stream, codec))
	}

	pub async fn recv<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
		trace!("waiting to receive json message");
		if let Some(next_message) = self.0.next().await {
			let byte_message = next_message.context("frame error, invalid byte length")?;
			let serialized_obj =
				serde_json::from_slice(&byte_message).context("unable to parse message")?;
			Ok(serialized_obj)
		} else {
			Ok(None)
		}
	}

	pub async fn recv_timeout<T: DeserializeOwned>(&mut self) -> Result<Option<T>> {
		timeout(NETWORK_TIMEOUT, self.recv())
			.await
			.context("timed out waiting for initial message")?
	}

	pub async fn send<T: Serialize>(&mut self, msg: T) -> Result<()> {
		trace!("sending json message");
		self.0.send(serde_json::to_string(&msg)?).await?;
		Ok(())
	}

	pub fn into_parts(self) -> FramedParts<U, AnyDelimiterCodec> {
		self.0.into_parts()
	}
}

pub async fn proxy<S1, S2>(stream1: S1, stream2: S2) -> io::Result<()>
	where
		S1: AsyncRead + AsyncWrite + Unpin,
		S2: AsyncRead + AsyncWrite + Unpin,
{
	let (mut s1_read, mut s1_write) = io::split(stream1);
	let (mut s2_read, mut s2_write) = io::split(stream2);
	tokio::select! {
        res = io::copy(&mut s1_read, &mut s2_write) => res,
        res = io::copy(&mut s2_read, &mut s1_write) => res,
    };
	Ok(())
}