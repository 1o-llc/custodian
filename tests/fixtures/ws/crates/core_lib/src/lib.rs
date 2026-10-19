//! The fixture's library: env reads, a mode enum, a code table, a config type.

/// The key the socket path is read from.
pub const ENV_SOCKET: &str = "FIX_SOCKET";

/// The fixture's own code table, in liaise's `liaise_codes!` shape (declared here so the fixture builds alone).
macro_rules! liaise_codes {
    (prefix: $p:expr, pub $name:ident { $($v:ident = $c:expr => $m:expr),* $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($v),* }
        impl $name {
            pub const PREFIX: &'static str = $p;
            pub fn code(self) -> u16 { match self { $(Self::$v => $c),* } }
            pub fn message(self) -> &'static str { match self { $(Self::$v => $m),* } }
        }
    };
}

liaise_codes! {
    prefix: "FIX",
    pub Code {
        NoSocket = 1 => "the socket path is not set",
        BadMode = 2 => "the mode is not serving, read_only or maintenance",
    }
}

pub fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

pub fn socket() -> Result<String, Code> {
    let s = env_or(ENV_SOCKET, "");
    if s.is_empty() {
        return Err(Code::NoSocket);
    }
    Ok(s)
}

pub fn token() -> Option<String> {
    std::env::var("FIX_TOKEN").ok()
}

#[derive(Debug, PartialEq, Eq)]
pub enum Mode {
    Serving,
    ReadOnly,
    Maintenance,
}

pub fn parse_mode(s: &str) -> Result<Mode, Code> {
    match s {
        "serving" => Ok(Mode::Serving),
        "read_only" => Ok(Mode::ReadOnly),
        "maintenance" => Ok(Mode::Maintenance),
        _ => Err(Code::BadMode),
    }
}

pub struct Config {
    pub name: String,
}

impl Config {
    pub fn new(name: &str) -> Self {
        Config { name: name.to_string() }
    }
}

pub fn serve_metrics(addr: &str) -> String {
    format!("serving metrics on {addr}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_parse() {
        assert_eq!(parse_mode("serving"), Ok(Mode::Serving));
        assert_eq!(parse_mode("nope"), Err(Code::BadMode));
    }

    #[test]
    fn config_names() {
        assert_eq!(Config::new("a").name, "a");
    }
}
