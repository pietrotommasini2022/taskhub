use anyhow::Result;
use inquire::{InquireError, Select};

pub async fn run() -> Result<()> {
    let db = crate::commands::init::db_path()?;
    if !db.exists() {
        println!("taskhub is not initialized. Run `taskhub init` first.");
        return Ok(());
    }

    let options = vec![
        "Run workflow",
        "New workflow",
        "Edit workflow",
        "View logs",
        "Validate workflow",
        "Manage secrets",
        "Manage plugins",
        "Start daemon",
        "Open dashboard",
        "Quit",
    ];

    let choice = match Select::new("What would you like to do?", options).prompt() {
        Ok(c) => c,
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => return Ok(()),
        Err(e) => return Err(e.into()),
    };

    match choice {
        "Run workflow" => run_interactive().await?,
        "New workflow" => new_interactive()?,
        "Edit workflow" => edit_interactive()?,
        "View logs" => logs_interactive()?,
        "Validate workflow" => validate_interactive()?,
        "Manage secrets" => secrets_interactive()?,
        "Manage plugins" => plugins_interactive()?,
        "Start daemon" => crate::commands::watch::run(false).await?,
        "Open dashboard" => crate::commands::dashboard::run().await?,
        "Quit" | _ => {}
    }
    Ok(())
}

fn pick_workflow(prompt: &str) -> Result<Option<String>> {
    let dir = crate::commands::init::taskhub_dir()?.join("workflows");
    let names = crate::resolve::list_workflow_names(&dir);
    if names.is_empty() {
        println!("No workflows found. Create one with `taskhub workflow new <name>`.");
        return Ok(None);
    }
    match Select::new(prompt, names).prompt() {
        Ok(n) => Ok(Some(n)),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

async fn run_interactive() -> Result<()> {
    if let Some(name) = pick_workflow("Select workflow to run:")? {
        let path = crate::resolve::resolve_workflow_path(&name)?;
        crate::commands::run::run(path.to_str().unwrap(), false).await?;
    }
    Ok(())
}

fn new_interactive() -> Result<()> {
    match inquire::Text::new("Workflow name:").prompt() {
        Ok(name) => crate::commands::workflow::new(&name),
        Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

fn edit_interactive() -> Result<()> {
    if let Some(name) = pick_workflow("Select workflow to edit:")? {
        crate::commands::workflow::open(Some(&name))?;
    }
    Ok(())
}

fn logs_interactive() -> Result<()> {
    if let Some(name) = pick_workflow("Select workflow:")? {
        crate::commands::logs::run(&name, None)?;
    }
    Ok(())
}

fn validate_interactive() -> Result<()> {
    if let Some(name) = pick_workflow("Select workflow to validate:")? {
        let path = crate::resolve::resolve_workflow_path(&name)?;
        crate::commands::validate::run(path.to_str().unwrap())?;
    }
    Ok(())
}

fn secrets_interactive() -> Result<()> {
    let options = vec!["Set secret", "List secrets", "Remove secret"];
    match Select::new("Secrets:", options).prompt() {
        Ok("Set secret") => {
            let key = inquire::Text::new("Key name:").prompt()?;
            crate::commands::secret::set(&key)?;
        }
        Ok("List secrets") => crate::commands::secret::list()?,
        Ok("Remove secret") => {
            let key = inquire::Text::new("Key name to remove:").prompt()?;
            crate::commands::secret::remove(&key)?;
        }
        Ok(_) | Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}

fn plugins_interactive() -> Result<()> {
    let options = vec!["List plugins", "Plugin info", "Remove plugin", "New plugin"];
    match Select::new("Plugins:", options).prompt() {
        Ok("List plugins") => crate::commands::plugin::list()?,
        Ok("Plugin info") => {
            let id = inquire::Text::new("Plugin ID:").prompt()?;
            crate::commands::plugin::info(&id)?;
        }
        Ok("Remove plugin") => {
            let id = inquire::Text::new("Plugin ID to remove:").prompt()?;
            crate::commands::plugin::remove(&id)?;
        }
        Ok("New plugin") => {
            let name = inquire::Text::new("Plugin name:").prompt()?;
            crate::commands::plugin::new_plugin(&name)?;
        }
        Ok(_) | Err(InquireError::OperationCanceled | InquireError::OperationInterrupted) => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
