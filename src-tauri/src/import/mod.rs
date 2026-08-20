//! Reading accounts out of the formats other tools produce.

mod otpauth;

pub use otpauth::{parse_otpauth, to_otpauth, ImportError};
