use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(args_conflicts_with_subcommands = true)]
pub(crate) struct Args {
    #[clap(subcommand)]
    pub(crate) action: Option<Action>,
}

#[derive(Subcommand)]
pub(crate) enum Action {
    Build {
        #[clap(long, default_value_t = false)]
        release: bool,
    },
    Npm {
        #[clap(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}
