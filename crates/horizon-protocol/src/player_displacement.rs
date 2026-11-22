//! An optional, ordered play extension for a server mod's physical player movement.
//! Unlike a teleport this carries no absolute position, look or velocity.
use crate::{Error, Result};

pub const CHANNEL: &str = "horizon:player_displacement";
pub const VERSION: &str = "1";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Displacement {
    pub sequence: u64,
    pub delta: [f64; 3],
}

pub fn encode(move_: Displacement) -> Vec<u8> {
    let mut body = Vec::with_capacity(32);
    body.extend(move_.sequence.to_be_bytes());
    for axis in move_.delta {
        body.extend(axis.to_be_bytes());
    }
    body
}

pub fn decode(body: &[u8]) -> Result<Displacement> {
    let bytes: &[u8; 32] = body
        .try_into()
        .map_err(|_| Error::new("invalid player displacement length"))?;
    let sequence = decode_ack(&bytes[..8])?;
    let delta = std::array::from_fn(|i| {
        let start = 8 * (i + 1);
        f64::from_be_bytes(bytes[start..start + 8].try_into().unwrap())
    });
    if !delta.iter().all(|d| d.is_finite()) {
        return Err(Error::new("nonfinite player displacement"));
    }
    Ok(Displacement { sequence, delta })
}

pub fn decode_ack(body: &[u8]) -> Result<u64> {
    let sequence = u64::from_be_bytes(
        body.try_into()
            .map_err(|_| Error::new("invalid player displacement acknowledgement length"))?,
    );
    if sequence == 0 {
        return Err(Error::new("invalid player displacement sequence"));
    }
    Ok(sequence)
}
