use std::io;
use std::net::SocketAddr;
use anyhow::{Result};
use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;
use dashmap::DashMap;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};
use tracing::{info, info_span, Instrument, warn};
use uuid::Uuid;
use crate::auth::Authenticator;
use crate::shared::{ClientMessage, CONTROL_PORT, Delimited, proxy, ServerMessage};

pub struct Server {
	port_range: RangeInclusive<u16>,
	auth: Option<Authenticator>,
	conns: Arc<DashMap<Uuid, TcpStream>>,
	server_port: u16
}

impl Server {
	pub fn new(server_port: u16, port_range: RangeInclusive<u16>, secret: Option<&str>) -> Self {
		assert!(!port_range.is_empty(), "must provide at least one port");
		Server {
			server_port,
			port_range,
			conns: Arc::new(DashMap::new()),
			auth: secret.map(Authenticator::new),
		}
	}

	pub async fn listen(self) -> Result<()> {
		let server_port = self.server_port.clone();
		let this = Arc::new(self);
		let addr = SocketAddr::from(([0, 0, 0, 0], server_port));
		let listener = TcpListener::bind(&addr).await?;
		info!(?addr, "server listening");

		loop {
			let (stream, addr) = listener.accept().await?;
			let this = Arc::clone(&this);
			tokio::spawn(
				async move {
					info!("incoming connection");
					if let Err(err) = this.handle_connection(stream).await {
						warn!(%err, "connection exited with error");
					} else {
						info!("connection exited");
					}
				}
					.instrument(info_span!("control", ?addr)),
			);
		}
	}

	async fn create_listener(&self, port: u16) -> Result<TcpListener, &'static str> {
		let try_bind = |port: u16| async move {
			TcpListener::bind(("0.0.0.0", port))
				.await
				.map_err(|err| match err.kind() {
					io::ErrorKind::AddrInUse => "port already in use",
					io::ErrorKind::PermissionDenied => "permission denied",
					_ => "failed to bind to port",
				})
		};
		if port > 0 {
			if !self.port_range.contains(&port) {
				return Err("client port number not in allowed range");
			}
			try_bind(port).await
		} else {
			for _ in 0..150 {
				let port = fastrand::u16(self.port_range.clone());
				match try_bind(port).await {
					Ok(listener) => return Ok(listener),
					Err(_) => continue,
				}
			}
			Err("failed to find an available port")
		}
	}

	async fn handle_connection(&self, stream: TcpStream) -> Result<()> {
		let mut stream = Delimited::new(stream);
		if let Some(auth) = &self.auth {
			if let Err(err) = auth.server_handshake(&mut stream).await {
				warn!(%err, "server handshake failed");
				stream.send(ServerMessage::Error(err.to_string())).await?;
				return Ok(());
			}
		}

		match stream.recv_timeout().await? {
			Some(ClientMessage::Authenticate(_)) => {
				warn!("unexpected authenticate");
				Ok(())
			}
			Some(ClientMessage::Hello(port)) => {
				let listener = match self.create_listener(port).await {
					Ok(listener) => listener,
					Err(err) => {
						stream.send(ServerMessage::Error(err.into())).await?;
						return Ok(());
					}
				};
				let port = listener.local_addr()?.port();
				info!(?port, "new client");
				stream.send(ServerMessage::Hello(port)).await?;

				loop {
					if stream.send(ServerMessage::Heartbeat).await.is_err() {
						return Ok(());
					}
					const TIMEOUT: Duration = Duration::from_millis(500);
					if let Ok(result) = timeout(TIMEOUT, listener.accept()).await {
						let (stream2, addr) = result?;
						info!(?addr, ?port, "new connection");

						let id = Uuid::new_v4();
						let conns = Arc::clone(&self.conns);

						conns.insert(id, stream2);
						tokio::spawn(async move {
							sleep(Duration::from_secs(10)).await;
							if conns.remove(&id).is_some() {
								warn!(%id, "removed stale connection");
							}
						});
						stream.send(ServerMessage::Connection(id)).await?;
					}
				}
			}
			Some(ClientMessage::Accept(id)) => {
				info!(%id, "forwarding connection");
				match self.conns.remove(&id) {
					Some((_, mut stream2)) => {
						let parts = stream.into_parts();
						debug_assert!(parts.write_buf.is_empty(), "framed write buffer not empty");
						stream2.write_all(&parts.read_buf).await?;
						proxy(parts.io, stream2).await?
					}
					None => warn!(%id, "missing connection"),
				}
				Ok(())
			}
			None => Ok(()),
		}
	}
}