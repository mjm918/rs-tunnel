use std::fmt::{Display, Formatter};
use std::fs;
use anyhow::{anyhow, Result};
use std::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
	pub server: Option<Server>,
	pub client: Option<Client>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Server {
	pub port: u16,
	pub secret: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Client {
	pub local: String,
	pub local_port: u16,
	pub server: String,
	pub port: u16,
	pub secret: String
}

fn cnf_path() -> String {
	Path::new(".")
		.join("tunnel.toml")
		.to_str()
		.unwrap()
		.to_string()
}

pub fn config() -> Result<Config> {
	let path = cnf_path();
	if Path::new(path.as_str()).exists() {
		let content = fs::read_to_string(path.as_str());
		if content.is_err() {
			return Err(anyhow!("failed to read config file"));
		}
		let content = content.unwrap();
		let cnf = toml::from_str::<Config>(content.as_str());
		if cnf.is_err() {
			return Err(anyhow!("failed to parse config file"));
		}
		let cnf = cnf.unwrap();
		return Ok(cnf);
	}
	Err(anyhow!("config not found"))
}
