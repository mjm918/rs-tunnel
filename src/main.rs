use std::ops::Deref;
use crate::config::{config};
use anyhow::Result;
use crate::client::Client;
use crate::server::Server;

pub mod shared;
pub mod auth;
pub mod server;
pub mod client;
pub mod config;

#[tokio::main]
async fn main() -> Result<()> {
    let config = config();
    if config.is_err() {
        panic!("{}",config.err().unwrap());
    }
    let config = config.unwrap();
    if let Some(cnf) = config.server {
        let port = cnf.port;
        let range = 80..=9099;
        Server::new(port,range, Some(cnf.secret.as_str()))
            .listen()
            .await?;
    }
    if let Some(cnf) = config.client {
        let client = Client::new(cnf.local.as_str(), cnf.local_port,cnf.server.as_str(), cnf.port, Some(cnf.secret.as_str())).await?;
        client.listen().await?;
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tokio::io;
    use crate::auth::Authenticator;
    use crate::shared::Delimited;

    #[tokio::test]
    async fn auth_handshake() -> Result<()> {
        let auth = Authenticator::new("some secret string");

        let (client, server) = io::duplex(8);
        let mut client = Delimited::new(client);
        let mut server = Delimited::new(server);

        tokio::try_join!(
            auth.client_handshake(&mut client),
            auth.server_handshake(&mut server),
        )?;

        Ok(())
    }
}