pub mod glyphs;
pub mod human;
pub mod json;

/// Output format selector.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Format {
    #[default]
    Human,
    Json,
}
