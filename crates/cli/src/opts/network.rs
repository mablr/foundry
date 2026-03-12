use clap::Parser;

#[derive(Debug, Parser)]
#[command(next_help_heading = "Networks")]
pub struct NetworkOpts {
    /// Enable Optimism network features
    #[arg(long, conflicts_with = "tempo")]
    pub optimism: bool,

    /// Enable Tempo network features
    #[arg(long, conflicts_with = "optimism")]
    pub tempo: bool,
}
