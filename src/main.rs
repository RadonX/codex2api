use clap::Parser;
use codex2api::{
    auth::{self, AuthManager},
    cli::{Cli, Command, ConfigCommand, KeyCommand},
    config::{self, Config},
    error::{AppError, Result},
    proxy::routes,
};
use secrecy::ExposeSecret;
use std::io::{self, Write};
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("error: {e}");
        std::process::exit(1)
    }
}
async fn run() -> Result<()> {
    let cli = Cli::parse();
    if matches!(
        cli.command,
        Command::Config {
            command: ConfigCommand::Path
        }
    ) {
        println!("{}", config::config_path()?.display());
        return Ok(());
    }
    match cli.command {
        Command::Init => init()?,
        Command::Login { no_open } => login(no_open).await?,
        Command::Serve { allow_non_loopback } => serve(allow_non_loopback).await?,
        Command::Status => status().await?,
        Command::Logout => logout().await?,
        Command::Key { command } => key(command)?,
        Command::Config { .. } => unreachable!(),
    }
    Ok(())
}
fn confirm(prompt: &str) -> Result<bool> {
    print!("{prompt} [y/N] ");
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
fn init() -> Result<()> {
    let path = config::config_path()?;
    config::ensure_dir(&config::data_dir()?)?;
    if path.exists() {
        if !confirm(&format!(
            "Configuration already exists at {}. Keep it and ensure a local key exists?",
            path.display()
        ))? {
            return Err(AppError::Config(
                "initialization cancelled; existing configuration was not replaced".into(),
            ));
        }
    } else {
        Config::default().save_new(&path)?
    }
    let cfg = Config::load()?;
    let store = auth::store::CredentialStore::new(&cfg.auth.credential_store)?;
    if store.local_key()?.is_none() {
        store.create_local_key(false)?;
    }
    println!("Initialized codex2api.\nServer URL: http://{}:{}/v1\nRetrieve the local API key with: codex2api key show",cfg.server.host,cfg.server.port);
    Ok(())
}
async fn login(no_open: bool) -> Result<()> {
    let cfg = Config::load()?;
    let manager = AuthManager::new(cfg.clone())?;
    if manager.configured().await&&!confirm("Account B credentials already exist. Replace them only after a new login fully succeeds?")?{return Err(AppError::Auth("login cancelled; existing Account B credentials were retained".into()))}
    let creds = auth::oauth::login(&cfg, no_open).await?;
    manager.replace(creds).await?;
    println!("Account B login succeeded; credentials were stored only in codex2api storage.");
    Ok(())
}
async fn serve(allow: bool) -> Result<()> {
    let cfg = Config::load()?;
    cfg.validate(allow)?;
    let level = cfg.logging.level.clone();
    tracing_subscriber::fmt()
        .with_env_filter(level)
        .with_target(false)
        .init();
    let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    println!("codex2api listening on http://{addr}");
    let app = routes::router(routes::AppState::new(cfg)?);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await
        .map_err(Into::into)
}
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("signal handler");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=term.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
async fn status() -> Result<()> {
    let cfg = Config::load()?;
    let manager = AuthManager::new(cfg.clone())?;
    match manager.store().load_credentials()?{Some(c)=>{let email=c.email.as_ref().map(|v|mask(v.expose_secret())).unwrap_or_else(||"unavailable".into());println!("Account B configured: yes\nIdentity: {email}\nAccess token expires: {}\nRefresh credential: yes",c.expires_at)},None=>println!("Account B configured: no\nIdentity: unavailable\nAccess token expires: unavailable\nRefresh credential: no")};
    println!(
        "Server address: http://{}:{}\nListener reachable: {}",
        cfg.server.host,
        cfg.server.port,
        tokio::time::timeout(
            std::time::Duration::from_millis(300),
            tokio::net::TcpStream::connect((cfg.server.host.as_str(), cfg.server.port))
        )
        .await
        .is_ok_and(|r| r.is_ok())
    );
    match manager.store().local_key()? {
        Some(k) => println!(
            "Local API-key fingerprint: {}",
            auth::store::CredentialStore::fingerprint(&k)
        ),
        None => println!("Local API-key fingerprint: unavailable"),
    };
    Ok(())
}
fn mask(s: &str) -> String {
    if let Some((a, b)) = s.split_once('@') {
        format!("{}***@{}", a.chars().next().unwrap_or('*'), b)
    } else {
        format!("{}***", s.chars().next().unwrap_or('*'))
    }
}
async fn logout() -> Result<()> {
    let cfg = Config::load()?;
    let manager = AuthManager::new(cfg)?;
    if manager.configured().await && confirm("Delete only codex2api Account B OAuth credentials?")?
    {
        manager.logout().await?;
        println!("Account B credentials removed. The local API key and personal Codex credentials were not changed.")
    } else {
        println!("No credentials removed.")
    }
    Ok(())
}
fn key(command: KeyCommand) -> Result<()> {
    let cfg = Config::load()?;
    let store = auth::store::CredentialStore::new(&cfg.auth.credential_store)?;
    match command {
        KeyCommand::Rotate => {
            if confirm(
                "Rotate the local API key now? Existing clients will immediately stop working.",
            )? {
                let k = store.create_local_key(true)?;
                println!(
                    "Local API key rotated. Fingerprint: {}",
                    auth::store::CredentialStore::fingerprint(&k)
                );
            }
        }
        KeyCommand::Show => {
            eprintln!(
                "Warning: the local API key grants access to Account B through this service."
            );
            if confirm("Display the local API key in this terminal?")? {
                let k = store.local_key()?.ok_or_else(|| {
                    AppError::Auth("local API key is not configured; run init".into())
                })?;
                println!("{}", k.expose_secret());
            }
        }
    }
    Ok(())
}
