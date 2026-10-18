use std::fmt;

/// Every failure the CLI can report, each mapped to a distinct exit code.
#[derive(Debug)]
pub enum Error {
    /// Could not connect to a Wayland compositor at all.
    Connect(String),
    /// A required KDE global is missing (not Plasma, or Plasma too old).
    ProtocolUnavailable(String),
    /// Wayland transport / dispatch failure.
    Protocol(String),
    /// No connected display matched the request.
    OutputNotFound { wanted: String, available: Vec<String> },
    /// `--output` omitted while several displays are connected.
    AmbiguousOutput(Vec<String>),
    /// The profile path is unusable (missing, not a file, ...).
    BadProfile(String),
    /// KWin refused the configuration.
    Rejected(String),
    /// Invalid user input (bad numbers, nothing to act on, ...).
    Invalid(String),
    /// Filesystem trouble in our own data directory.
    Io(String),
}

impl Error {
    pub fn exit_code(&self) -> u8 {
        match self {
            Error::Connect(_) | Error::Protocol(_) => 1,
            Error::ProtocolUnavailable(_) => 2,
            Error::OutputNotFound { .. } | Error::AmbiguousOutput(_) => 3,
            Error::BadProfile(_) => 4,
            Error::Rejected(_) => 5,
            Error::Invalid(_) => 6,
            Error::Io(_) => 7,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Connect(m) => write!(f, "cannot connect to Wayland: {m}"),
            Error::ProtocolUnavailable(m) => write!(f, "{m}"),
            Error::Protocol(m) => write!(f, "Wayland protocol error: {m}"),
            Error::OutputNotFound { wanted, available } => write!(
                f,
                "no display matches '{wanted}' (available: {})",
                available.join(", ")
            ),
            Error::AmbiguousOutput(a) => write!(
                f,
                "several displays are connected, pass --output (available: {})",
                a.join(", ")
            ),
            Error::BadProfile(m) => write!(f, "{m}"),
            Error::Rejected(m) => write!(f, "KDE rejected the configuration: {m}"),
            Error::Invalid(m) => write!(f, "{m}"),
            Error::Io(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<crate::curves::CurveError> for Error {
    fn from(e: crate::curves::CurveError) -> Self {
        Error::Invalid(e.to_string())
    }
}

impl From<crate::icc::IccError> for Error {
    fn from(e: crate::icc::IccError) -> Self {
        Error::BadProfile(e.0)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}
