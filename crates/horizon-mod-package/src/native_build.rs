//! Installed native build provenance, shared by the launcher and control CLI.
//! This record locates a build's base resources; it is not runtime mod identity.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Client,
    Server,
}

/// The required, validated fields of an installed `native-mods.json` record.
#[derive(Debug, PartialEq, Eq)]
pub struct Record<'a> {
    pub base: &'a str,
    pub horizon: &'a str,
    pub target: Target,
}

impl<'a> Record<'a> {
    pub fn from_json(json: &'a Value) -> Option<Self> {
        let horizon = json["horizon"].as_str()?;
        if horizon.len() != 40
            || !horizon
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        {
            return None;
        }
        Some(Self {
            base: json["base"].as_str()?,
            horizon,
            target: match json["target"].as_str()? {
                "client" => Target::Client,
                "server" => Target::Server,
                _ => return None,
            },
        })
    }
}
