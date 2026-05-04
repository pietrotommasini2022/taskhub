mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "taskhub", version, about = "Personal automation runtime")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize ~/.taskhub/ and create base config
    Init,
    /// Run a workflow file once
    Run {
        workflow: String,
    },
    /// Start daemon, run all active workflows
    Watch {
        #[arg(long)]
        tray: bool,
    },
    /// List recent workflow runs
    List,
    /// Tail logs for a workflow
    Logs {
        workflow: String,
        #[arg(long)]
        run: Option<String>,
    },
    /// Dry-run a workflow (no side effects, no storage writes)
    Test {
        workflow: String,
    },
    /// Validate workflow schema without executing
    Validate {
        workflow: String,
    },
    /// Manage secrets
    Secret {
        #[command(subcommand)]
        action: SecretCommand,
    },
    /// Manage plugins
    Plugin {
        #[command(subcommand)]
        action: PluginCommand,
    },
    /// Open local read-only dashboard
    Dashboard,
}

#[derive(Subcommand)]
enum SecretCommand {
    /// Store a secret (prompts for value)
    Set { key: String },
    /// List secret keys (values never shown)
    List,
    /// Remove a secret
    Rm { key: String },
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Install a plugin from a git URL or registry
    Install { source: String },
    /// List installed plugins
    List,
    /// Show plugin details
    Info { id: String },
    /// Remove a plugin
    Rm { id: String },
    /// Scaffold a new plugin
    New { name: String },
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Init => commands::init::run()?,
        Command::Run { workflow } => commands::run::run(&workflow, false).await?,
        Command::Test { workflow } => commands::run::run(&workflow, true).await?,
        Command::Validate { workflow } => commands::validate::run(&workflow)?,
        Command::List => commands::list::run()?,
        Command::Logs { workflow, run } => commands::logs::run(&workflow, run.as_deref())?,

        Command::Watch { tray } => commands::watch::run(tray).await?,
        Command::Secret { action } => match action {
            SecretCommand::Set { key } => commands::secret::set(&key)?,
            SecretCommand::List => commands::secret::list()?,
            SecretCommand::Rm { key } => commands::secret::remove(&key)?,
        },
        Command::Plugin { action } => match action {
            PluginCommand::Install { source } => commands::plugin::install(&source)?,
            PluginCommand::List => commands::plugin::list()?,
            PluginCommand::Info { id } => commands::plugin::info(&id)?,
            PluginCommand::Rm { id } => commands::plugin::remove(&id)?,
            PluginCommand::New { name } => commands::plugin::new_plugin(&name)?,
        },
        Command::Dashboard => commands::dashboard::run().await?,
    }

    Ok(())
}
