//! Minimal riptide-rs command-line entry point.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use directories::ProjectDirs;
use riptide_core::CatalogEntity;
use riptide_provider_tidal::{
    TidalApiCache, TidalAuthClient, TidalClient, TidalCredentials, TidalError, TidalTokenStore,
};
use riptide_template::{context_from_fixture_str, CompiledTemplate, TemplateContext};
use std::{
    fs,
    path::{Path, PathBuf},
};
use url::Url;

const DEFAULT_AUTH_BASE_URL: &str = "https://auth.tidal.com/v1/oauth2/";
const DEFAULT_API_BASE_URL: &str = "https://api.tidal.com/v1/";

/// riptide-rs command-line interface.
#[derive(Debug, Parser)]
#[command(version, about = "Rust implementation of riptide")]
struct Cli {
    /// Path to the TIDAL token JSON file.
    #[arg(long, global = true)]
    auth_file: Option<PathBuf>,

    /// Path to the TIDAL API cache database.
    #[arg(long, global = true)]
    cache_file: Option<PathBuf>,

    /// TIDAL auth base URL.
    #[arg(long, global = true, default_value = DEFAULT_AUTH_BASE_URL)]
    auth_base_url: Url,

    /// TIDAL API base URL.
    #[arg(long, global = true, default_value = DEFAULT_API_BASE_URL)]
    api_base_url: Url,

    #[command(subcommand)]
    command: Option<Command>,
}

/// Top-level commands.
#[derive(Debug, Clone, Subcommand)]
enum Command {
    /// Validate a configuration file.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Compile and render path templates.
    Template {
        #[command(subcommand)]
        command: TemplateCommand,
    },
    /// Manage TIDAL authentication.
    Auth {
        #[command(subcommand)]
        command: AuthCommand,
    },
    /// Read catalog data from TIDAL.
    List {
        #[command(subcommand)]
        command: ListCommand,
    },
}

/// Configuration commands.
#[derive(Debug, Clone, Subcommand)]
enum ConfigCommand {
    /// Check whether a config file parses and validates.
    Check {
        /// Path to the TOML config file. Required in this slice.
        #[arg(long)]
        path: PathBuf,
    },
}

/// Template commands.
#[derive(Debug, Clone, Subcommand)]
enum TemplateCommand {
    /// Check whether a template parses.
    Check {
        /// Template string.
        template: String,
    },
    /// Render a template using a JSON fixture.
    RenderFixture {
        /// Template string.
        template: String,
        /// Path to the JSON fixture.
        #[arg(long)]
        fixture: PathBuf,
        /// Do not append `.*`.
        #[arg(long)]
        no_asterisk_ext: bool,
    },
}

/// Authentication commands.
#[derive(Debug, Clone, Subcommand)]
enum AuthCommand {
    /// Show whether a TIDAL token is available.
    Status,
    /// Run OAuth device authorization.
    Login,
    /// Refresh the stored token.
    Refresh,
    /// Logout and clear the stored token.
    Logout,
}

/// List commands.
#[derive(Debug, Clone, Subcommand)]
enum ListCommand {
    /// Fetch a track by ID.
    Track(ListOne),
    /// Fetch a video by ID.
    Video(ListOne),
    /// Fetch an album by ID.
    Album(ListOne),
    /// Fetch a playlist by ID.
    Playlist(ListOne),
    /// Search the catalog.
    Search {
        /// Search query.
        query: String,
        /// Optional template to render each row.
        #[arg(long)]
        template: Option<String>,
    },
}

/// Shared single-resource list options.
#[derive(Debug, Clone, Args)]
struct ListOne {
    /// TIDAL resource ID.
    id: String,
    /// Optional template to render.
    #[arg(long)]
    template: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command.clone() {
        Some(Command::Config {
            command: ConfigCommand::Check { path },
        }) => check_config(path),
        Some(Command::Template { command }) => run_template(command),
        Some(Command::Auth { command }) => run_auth(&cli, command).await,
        Some(Command::List { command }) => run_list(&cli, command).await,
        None => Ok(()),
    }
}

fn check_config(path: PathBuf) -> Result<()> {
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to load config from {}", path.display()))?;
    riptide_config::ValidatedConfig::from_toml_str(&contents)
        .with_context(|| format!("failed to validate config from {}", path.display()))?;
    println!("config ok: {}", path.display());
    Ok(())
}

fn run_template(command: TemplateCommand) -> Result<()> {
    match command {
        TemplateCommand::Check { template } => {
            CompiledTemplate::parse(&template).context("failed to compile template")?;
            println!("template ok");
            Ok(())
        }
        TemplateCommand::RenderFixture {
            template,
            fixture,
            no_asterisk_ext,
        } => {
            let compiled =
                CompiledTemplate::parse(&template).context("failed to compile template")?;
            let fixture_json = fs::read_to_string(&fixture)
                .with_context(|| format!("failed to load fixture from {}", fixture.display()))?;
            let context =
                context_from_fixture_str(&fixture_json).context("failed to decode fixture")?;
            let rendered = compiled
                .render_with_options(
                    &context,
                    riptide_template::TemplateRenderOptions {
                        with_asterisk_ext: !no_asterisk_ext,
                    },
                )
                .context("failed to render template")?;
            println!("{}", rendered.as_str());
            Ok(())
        }
    }
}

async fn run_auth(cli: &Cli, command: AuthCommand) -> Result<()> {
    let token_store = TidalTokenStore::new(auth_file(cli)?);
    match command {
        AuthCommand::Status => {
            match token_store
                .load()
                .await
                .context("failed to load token store")?
            {
                Some(token) => println!(
                    "authenticated: user={} country={} expires_at={}",
                    token.user_id.as_str(),
                    token.country_code.as_str(),
                    token.expires_at.as_datetime()
                ),
                None => println!("not authenticated"),
            }
            Ok(())
        }
        AuthCommand::Login => {
            let auth = auth_client(cli)?;
            let authorization = auth
                .device_authorization()
                .await
                .context("failed to start TIDAL device authorization")?;
            println!("Open: {}", authorization.verification_uri_complete);
            println!("Code: {}", authorization.user_code.as_str());
            let mut interval = authorization.interval;
            loop {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        eprintln!("auth login cancelled");
                        std::process::exit(130);
                    }
                    _ = tokio::time::sleep(interval.as_duration()) => {}
                }
                match auth.exchange_device_code(&authorization.device_code).await {
                    Ok(token) => {
                        token_store
                            .save(&token)
                            .await
                            .context("failed to save TIDAL token")?;
                        println!("authenticated: user={}", token.user_id.as_str());
                        return Ok(());
                    }
                    Err(TidalError::Api { message, .. })
                        if message.contains("authorization_pending") =>
                    {
                        continue;
                    }
                    Err(TidalError::Api { message, .. }) if message.contains("slow_down") => {
                        interval = interval.slow_down();
                    }
                    Err(error) => return Err(error).context("TIDAL device authorization failed"),
                }
            }
        }
        AuthCommand::Refresh => {
            let Some(token) = token_store
                .load()
                .await
                .context("failed to load token store")?
            else {
                anyhow::bail!("not authenticated");
            };
            let refreshed = auth_client(cli)?
                .refresh(&token.refresh_token)
                .await
                .context("failed to refresh TIDAL token")?;
            token_store
                .save(&refreshed)
                .await
                .context("failed to save refreshed TIDAL token")?;
            println!("token refreshed");
            Ok(())
        }
        AuthCommand::Logout => {
            if let Some(token) = token_store
                .load()
                .await
                .context("failed to load token store")?
            {
                auth_client(cli)?
                    .logout(&token.access_token)
                    .await
                    .context("failed to logout from TIDAL")?;
            }
            token_store
                .clear()
                .await
                .context("failed to clear token store")?;
            println!("logged out");
            Ok(())
        }
    }
}

async fn run_list(cli: &Cli, command: ListCommand) -> Result<()> {
    let client = tidal_client(cli).await?;
    match command {
        ListCommand::Track(options) => {
            let template = compile_optional_template(options.template)?;
            let track = client
                .track(&options.id)
                .await
                .context("failed to fetch track")?;
            print_entity(CatalogEntity::Track(track), template.as_ref())
        }
        ListCommand::Video(options) => {
            let template = compile_optional_template(options.template)?;
            let video = client
                .video(&options.id)
                .await
                .context("failed to fetch video")?;
            print_entity(CatalogEntity::Video(video), template.as_ref())
        }
        ListCommand::Album(options) => {
            let template = compile_optional_template(options.template)?;
            let album = client
                .album(&options.id)
                .await
                .context("failed to fetch album")?;
            print_entity(CatalogEntity::Album(album), template.as_ref())
        }
        ListCommand::Playlist(options) => {
            let template = compile_optional_template(options.template)?;
            let playlist = client
                .playlist(&options.id)
                .await
                .context("failed to fetch playlist")?;
            print_entity(CatalogEntity::Playlist(playlist), template.as_ref())
        }
        ListCommand::Search { query, template } => {
            let template = compile_optional_template(template)?;
            let results = client
                .search(&query)
                .await
                .context("failed to search TIDAL")?;
            if let Some(top_hit) = results.top_hit {
                print_grouped_entity("top_hit", top_hit, template.as_ref())?;
            }
            for track in results.tracks {
                print_grouped_entity("track", CatalogEntity::Track(track), template.as_ref())?;
            }
            for album in results.albums {
                print_grouped_entity("album", CatalogEntity::Album(album), template.as_ref())?;
            }
            for playlist in results.playlists {
                print_grouped_entity(
                    "playlist",
                    CatalogEntity::Playlist(playlist),
                    template.as_ref(),
                )?;
            }
            for artist in results.artists {
                print_grouped_entity("artist", CatalogEntity::Artist(artist), template.as_ref())?;
            }
            for video in results.videos {
                print_grouped_entity("video", CatalogEntity::Video(video), template.as_ref())?;
            }
            Ok(())
        }
    }
}

fn compile_optional_template(template: Option<String>) -> Result<Option<CompiledTemplate>> {
    template
        .map(|template| CompiledTemplate::parse(&template).context("failed to compile template"))
        .transpose()
}

fn print_grouped_entity(
    group: &str,
    entity: CatalogEntity,
    template: Option<&CompiledTemplate>,
) -> Result<()> {
    if template.is_some() {
        return print_entity(entity, template);
    }
    println!("{group}\t{}", entity_row(&entity));
    Ok(())
}

fn print_entity(entity: CatalogEntity, template: Option<&CompiledTemplate>) -> Result<()> {
    if let Some(template) = template {
        let context = context_for_entity(&entity);
        let rendered = template
            .render(&context)
            .context("failed to render template")?;
        println!("{}", rendered.as_str());
    } else {
        println!("{}", entity_row(&entity));
    }
    Ok(())
}

fn context_for_entity(entity: &CatalogEntity) -> TemplateContext {
    let now = time::OffsetDateTime::now_utc();
    match entity {
        CatalogEntity::Track(track) => TemplateContext::new(now).with_track(track),
        CatalogEntity::Video(video) => TemplateContext::new(now).with_video(video),
        CatalogEntity::Album(album) => TemplateContext::new(now).with_album(album),
        CatalogEntity::Playlist(playlist) => {
            TemplateContext::new(now).with_playlist_input(riptide_template::PlaylistTemplateInput {
                uuid: playlist.id.id.as_str().to_owned(),
                title: playlist.title.clone(),
                index: 1,
                created: playlist.created_at,
                updated: playlist.updated_at,
            })
        }
        CatalogEntity::Artist(_) | CatalogEntity::Collection(_) => TemplateContext::new(now),
    }
}

fn entity_row(entity: &CatalogEntity) -> String {
    match entity {
        CatalogEntity::Track(track) => format!("track\t{}\t{}", track.id.id.as_str(), track.title),
        CatalogEntity::Video(video) => format!("video\t{}\t{}", video.id.id.as_str(), video.title),
        CatalogEntity::Album(album) => format!("album\t{}\t{}", album.id.id.as_str(), album.title),
        CatalogEntity::Playlist(playlist) => {
            format!("playlist\t{}\t{}", playlist.id.id.as_str(), playlist.title)
        }
        CatalogEntity::Artist(artist) => format!(
            "artist\t{}\t{}",
            artist.id.as_ref().map(|id| id.id.as_str()).unwrap_or(""),
            artist.name
        ),
        CatalogEntity::Collection(collection) => format!(
            "collection\t{}\t{}",
            collection.id.id.as_str(),
            collection.title.as_deref().unwrap_or("")
        ),
    }
}

async fn tidal_client(cli: &Cli) -> Result<TidalClient> {
    let auth = auth_client(cli)?;
    let token_store = TidalTokenStore::new(auth_file(cli)?);
    let cache = TidalApiCache::open(cache_file(cli)?)
        .await
        .context("failed to open TIDAL cache")?;
    TidalClient::try_new(cli.api_base_url.clone(), auth, token_store, cache)
        .context("failed to build TIDAL API client")
}

fn auth_client(cli: &Cli) -> Result<TidalAuthClient> {
    let credentials = TidalCredentials::from_env().context("failed to load TIDAL credentials")?;
    TidalAuthClient::try_new(cli.auth_base_url.clone(), credentials)
        .context("failed to build TIDAL auth client")
}

fn auth_file(cli: &Cli) -> Result<PathBuf> {
    cli.auth_file
        .clone()
        .or_else(|| project_dirs().map(|dirs| dirs.data_local_dir().join("tidal-token.json")))
        .context("failed to determine auth file path")
}

fn cache_file(cli: &Cli) -> Result<PathBuf> {
    cli.cache_file
        .clone()
        .or_else(|| project_dirs().map(|dirs| dirs.cache_dir().join("tidal-cache.redb")))
        .context("failed to determine cache file path")
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "riptide", "riptide-rs")
}

#[allow(dead_code)]
fn assert_path_is_local(path: &Path) -> &Path {
    path
}
