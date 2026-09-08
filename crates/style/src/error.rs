pub type Result<T> = std::result::Result<T, StyleError>;

#[derive(Debug, thiserror::Error)]
pub enum StyleError {
    #[error("Invalid source line")]
    SourceLine,
    #[error("Missing source line")]
    MissingSourceLine,
    #[error("Invalid source column")]
    SourceColumn,
    #[error("Overlapping source edits")]
    OverlappingEdits,
    #[error("Invalid {0} boundary")]
    Boundary(&'static str),
    #[error("Invalid literal span")]
    LiteralSpan,
    #[error("Unterminated block comment at byte {0}")]
    BlockComment(usize),
    #[error("Unterminated WGSL block comment at byte {0}")]
    WgslBlockComment(usize),
    #[error("Unterminated quoted token at byte {0}")]
    QuotedToken(usize),
    #[error("Rust tokenization failed: {0}")]
    Tokenization(String),
    #[error("Parsing Rust failed")]
    RustParse(#[from] syn::Error),
    #[error("{operation}")]
    Io {
        operation: &'static str,
        #[source]
        source: std::io::Error,
    },
    #[error("Opening rustfmt input failed")]
    RustfmtInput,
    #[error("Rustfmt input writer failed")]
    RustfmtWriter,
    #[error("Rustfmt failed: {0}")]
    Rustfmt(String),
    #[error("Reading rustfmt output failed")]
    RustfmtOutput(#[from] std::string::FromUtf8Error),
    #[error("Cargo metadata failed: {0}")]
    CargoMetadata(String),
    #[error("Invalid Cargo metadata")]
    MetadataJson(#[from] serde_json::Error),
    #[error("Missing {0}")]
    MissingMetadata(&'static str),
    #[error("Path is outside the workspace")]
    WorkspacePath(#[from] std::path::StripPrefixError),
    #[error("Staged path is not UTF-8")]
    StagedUtf8(#[from] std::str::Utf8Error),
    #[error("Staged paths must be relative to the repository")]
    StagedPath,
    #[error("Supported source types are rs and wgsl")]
    SourceType,
    #[error("Standard input violates source style")]
    StdinStyle,
    #[error("No Rust or WGSL source files found")]
    NoSources,
    #[error("{0} structural violation(s) require manual changes")]
    Structure(usize),
    #[error("{0} file(s) need cargo style fix")]
    Formatting(usize),
}
