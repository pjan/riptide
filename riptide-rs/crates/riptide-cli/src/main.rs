//! Minimal riptide-rs command-line entry point.

mod progress;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use directories::{ProjectDirs, UserDirs};
use progress::{tui_sink::TuiRenderOpts, ColorMode, MediaTransferProgress, TuiSink};
use riptide_config::{
    Concurrency, DownloadConfig, M3uConfig, MetadataConfig, SinglesMode, ValidatedConfig,
};
use riptide_core::{
    Album, AudioQualityRequest, CatalogEntity, CollectionKind, CollectionProvider, CoverArtId,
    LyricsProvider, MediaDuration, MediaRef, ProviderQualityLabel, QualityRequest, Resolver,
    ResourceId, StreamProvider, Track,
};
use riptide_media::{
    download_segments, plan_download, post_process, resolve_manifest_async, DownloadOptions,
};
use riptide_metadata::{
    metadata_from_track, save_cover, write_lyrics_sidecar, write_m3u, write_tags, M3uEntry,
};
use riptide_progress::{
    label::{label_for_album, label_for_track, quality_badge_for_stream},
    ArtifactKind, ArtifactOutcome, DisplayLabel, DownloadEvent, DownloadPhase, GroupId,
    GroupOutcome, ItemId, ItemKind, ItemOutcome, OperationConfig, OperationId, OperationKind,
    ProgressIdAllocator, ProgressSink,
};
use riptide_provider_tidal::{
    cover_url, CoverArtSize, FileTokenStore, HttpClient, RedbApiCache, TidalAuthClient,
    TidalClient, TidalCredentials, TidalError, TidalProvider, TokenStore,
};
use riptide_template::{
    context_from_fixture_str, CompiledTemplate, TemplateContext, TemplateRenderOptions,
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::task::JoinSet;
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

    /// Path to the TOML configuration file.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// TIDAL auth base URL.
    #[arg(long, global = true, default_value = DEFAULT_AUTH_BASE_URL, hide = true)]
    auth_base_url: Url,

    /// TIDAL API base URL.
    #[arg(long, global = true, default_value = DEFAULT_API_BASE_URL, hide = true)]
    api_base_url: Url,

    #[command(subcommand)]
    command: Command,
}

/// Top-level commands.
#[derive(Debug, Subcommand)]
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
    /// Download media from TIDAL.
    Download(DownloadArgs),
}

/// Configuration commands.
#[derive(Debug, Subcommand)]
enum ConfigCommand {
    /// Check whether a config file parses and validates.
    Check {
        /// Path to the TOML config file. Required in this slice.
        #[arg(long)]
        path: PathBuf,
    },
}

/// Template commands.
#[derive(Debug, Subcommand)]
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
#[derive(Debug, Subcommand)]
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
#[derive(Debug, Subcommand)]
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

/// Global options shared across commands that need TIDAL access.
struct SharedOptions {
    auth_file: Option<PathBuf>,
    cache_file: Option<PathBuf>,
    config_file: Option<PathBuf>,
    auth_base_url: Url,
    api_base_url: Url,
}

/// Download command options.
#[derive(Debug, Args)]
struct DownloadArgs {
    /// TIDAL URLs or resource references.
    urls: Vec<String>,
    /// Override audio quality.
    #[arg(long)]
    audio_quality: Option<String>,
    /// Override artist singles handling.
    #[arg(long, value_enum)]
    singles: Option<SinglesModeArg>,
    /// Override output template.
    #[arg(long)]
    output: Option<String>,
    /// Override download root.
    #[arg(long)]
    path: Option<PathBuf>,
    /// Override concurrency.
    #[arg(long)]
    concurrency: Option<u8>,
    /// Existing-file behavior.
    #[command(flatten)]
    existing: ExistingFlags,
    /// Metadata behavior.
    #[command(flatten)]
    metadata: MetadataFlags,
    /// Error behavior.
    #[command(flatten)]
    errors: ErrorFlags,
    /// Progress display mode.
    #[arg(long, value_enum, default_value = "auto")]
    progress: ProgressMode,
    /// ANSI colour mode.
    #[arg(long, value_enum, default_value = "auto")]
    color: ColorMode,
}

/// Singles handling override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum SinglesModeArg {
    /// Include albums, singles, and EPs.
    Include,
    /// Exclude singles and EPs from artist downloads.
    Exclude,
    /// Download only singles and EPs from artist downloads.
    Only,
}

impl From<SinglesModeArg> for SinglesMode {
    fn from(value: SinglesModeArg) -> Self {
        match value {
            SinglesModeArg::Include => Self::Include,
            SinglesModeArg::Exclude => Self::Exclude,
            SinglesModeArg::Only => Self::Only,
        }
    }
}

/// Existing-file flags.
#[derive(Debug, Args)]
struct ExistingFlags {
    /// Skip existing files.
    #[arg(long)]
    skip_existing: bool,
    /// Overwrite existing files.
    #[arg(long)]
    no_skip_existing: bool,
}

/// Metadata flags.
#[derive(Debug, Args)]
struct MetadataFlags {
    /// Skip metadata tagging.
    #[arg(long)]
    no_metadata: bool,
    /// Skip lyrics fetch.
    #[arg(long)]
    no_lyrics: bool,
    /// Skip cover art.
    #[arg(long)]
    no_cover: bool,
}

/// Error behavior flags.
#[derive(Debug, Args)]
struct ErrorFlags {
    /// Continue downloading when individual items fail.
    #[arg(long)]
    skip_errors: bool,
}

/// Download progress display mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ProgressMode {
    /// Show progress bars when stderr is an interactive terminal.
    Auto,
    /// Always show progress bars.
    Always,
    /// Disable progress bars and print simple status lines.
    Never,
}

#[derive(Clone)]
struct EffectiveDownloadOptions {
    root: PathBuf,
    audio_quality: riptide_core::AudioQualityRequest,
    skip_existing: bool,
    concurrency: usize,
    template: CompiledTemplate,
    metadata: MetadataConfig,
    m3u: M3uConfig,
    singles_mode: SinglesMode,
    skip_errors: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlbumType {
    Album,
    Single,
    Ep,
    Unknown,
}

impl AlbumType {
    fn from_release_type(release_type: Option<&str>) -> Self {
        match release_type {
            Some("ALBUM" | "COMPILATION") => Self::Album,
            Some("SINGLE") => Self::Single,
            Some("EP") => Self::Ep,
            _ => Self::Unknown,
        }
    }

    const fn is_single_or_ep(self) -> bool {
        matches!(self, Self::Single | Self::Ep)
    }
}

#[derive(Debug, Default)]
struct ExpansionResult {
    /// Flat items for non-grouped operations (album, playlist, track, mix).
    items: Vec<(CatalogEntity, Option<Album>)>,
    /// Album groups for artist operations. When non-empty, `items` is empty.
    groups: Vec<ExpandedGroup>,
    summary: DownloadSummary,
}

impl ExpansionResult {
    #[allow(dead_code)]
    const fn is_grouped(&self) -> bool {
        !self.groups.is_empty()
    }

    fn total_items(&self) -> usize {
        self.items.len() + self.groups.iter().map(|g| g.items.len()).sum::<usize>()
    }

    fn assert_valid_shape(&self) {
        debug_assert!(
            self.items.is_empty() || self.groups.is_empty(),
            "ExpansionResult must use items or groups, not both"
        );
    }
}

struct ExpandedGroup {
    album: Album,
    label: DisplayLabel,
    items: Vec<CatalogEntity>,
}

impl std::fmt::Debug for ExpandedGroup {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExpandedGroup")
            .field("album", &self.album.title)
            .field("items_count", &self.items.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Default, Clone)]
struct DownloadSummary {
    downloaded: usize,
    skipped: usize,
    failed_items: usize,
    failed_expansions: usize,
    failure_messages: Vec<String>,
}

impl DownloadSummary {
    fn merge(&mut self, other: Self) {
        self.downloaded += other.downloaded;
        self.skipped += other.skipped;
        self.failed_items += other.failed_items;
        self.failed_expansions += other.failed_expansions;
        self.failure_messages.extend(other.failure_messages);
    }

    const fn has_failures(&self) -> bool {
        self.failed_items > 0 || self.failed_expansions > 0
    }
}

struct ScheduledDownload {
    index: usize,
    entity: CatalogEntity,
    item_ref: MediaRef,
    parent_album: Option<Album>,
    item_id: ItemId,
    operation_id: OperationId,
    group_id: Option<GroupId>,
    item_label: DisplayLabel,
}

struct DownloadTaskOutput {
    index: usize,
    group_id: Option<GroupId>,
    completion: ItemCompletion,
}

struct DownloadTaskFailure {
    item_id: ItemId,
    group_id: Option<GroupId>,
    item_label: DisplayLabel,
    item_ref: MediaRef,
    error: anyhow::Error,
}

impl DownloadTaskFailure {
    fn message(&self) -> String {
        format!(
            "{} ({}) failed: {:#}",
            display_label_text(&self.item_label),
            media_ref_label(&self.item_ref),
            self.error
        )
    }
}

struct GroupCompletionTracker {
    groups: HashMap<GroupId, GroupCompletionState>,
}

struct GroupCompletionState {
    total: u64,
    finished: u64,
    failed: u64,
}

impl GroupCompletionTracker {
    fn new() -> Self {
        Self {
            groups: HashMap::new(),
        }
    }

    fn register(&mut self, group_id: GroupId, total: usize) {
        self.groups.insert(
            group_id,
            GroupCompletionState {
                total: total as u64,
                finished: 0,
                failed: 0,
            },
        );
    }

    /// Records an item completion. Returns `Some((group_id, outcome))` when the
    /// group is fully finished.
    fn record_item_finished(
        &mut self,
        group_id: Option<GroupId>,
        failed: bool,
    ) -> Option<(GroupId, GroupOutcome)> {
        let group_id = group_id?;
        let state = self.groups.get_mut(&group_id)?;
        state.finished += 1;
        if failed {
            state.failed += 1;
        }
        if state.finished >= state.total {
            let outcome = if state.failed == 0 {
                GroupOutcome::Done
            } else {
                GroupOutcome::Partial {
                    failed: state.failed,
                }
            };
            Some((group_id, outcome))
        } else {
            None
        }
    }
}

enum ItemCompletion {
    Downloaded(Option<M3uEntry>),
    Skipped(Option<M3uEntry>),
}

/// Shared single-resource list options.
#[derive(Debug, Args)]
struct ListOne {
    /// TIDAL resource ID.
    id: String,
    /// Optional template to render.
    #[arg(long)]
    template: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let Cli {
        auth_file,
        cache_file,
        config,
        auth_base_url,
        api_base_url,
        command,
    } = Cli::parse();
    let options = SharedOptions {
        auth_file,
        cache_file,
        config_file: config,
        auth_base_url,
        api_base_url,
    };
    match command {
        Command::Config {
            command: ConfigCommand::Check { path },
        } => check_config(&path),
        Command::Template { command } => run_template(command),
        Command::Auth { command } => run_auth(&options, command).await,
        Command::List { command } => run_list(&options, command).await,
        Command::Download(args) => {
            let summary = run_download(&options, args).await?;
            if summary.has_failures() {
                eprintln!(
                    "download completed with {} failed item(s) and {} failed expansion(s)",
                    summary.failed_items, summary.failed_expansions
                );
                print_failure_messages(&summary.failure_messages, 20);
                std::process::exit(1);
            }
            Ok(())
        }
    }
}

fn check_config(path: &Path) -> Result<()> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("failed to load config from {}", path.display()))?;
    riptide_config::ValidatedConfig::from_toml_str(&contents)
        .with_context(|| format!("failed to validate config from {}", path.display()))?;
    println!("config ok: {}", path.display());
    Ok(())
}

fn run_template(command: TemplateCommand) -> Result<()> {
    let template_str = match &command {
        TemplateCommand::Check { template } | TemplateCommand::RenderFixture { template, .. } => {
            template.clone()
        }
    };
    let compiled =
        CompiledTemplate::parse(&template_str).context("failed to compile template")?;
    match command {
        TemplateCommand::Check { .. } => println!("template ok"),
        TemplateCommand::RenderFixture {
            fixture,
            no_asterisk_ext,
            ..
        } => {
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
        }
    }
    Ok(())
}

async fn run_auth(options: &SharedOptions, command: AuthCommand) -> Result<()> {
    let token_store = FileTokenStore::new(auth_file(options)?);
    let http = http_client()?;
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
            let auth = auth_client(http.clone(), options)?;
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
                    () = tokio::time::sleep(interval.as_duration()) => {}
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
                    Err(TidalError::AuthorizationPending) => {}
                    Err(TidalError::SlowDown) => {
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
            let refreshed = auth_client(http.clone(), options)?
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
                auth_client(http.clone(), options)?
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

async fn run_list(options: &SharedOptions, command: ListCommand) -> Result<()> {
    let client = tidal_client(options).await?;
    match command {
        ListCommand::Track(options) => {
            let template = compile_optional_template(options.template)?;
            let id = list_resource_id(&options.id)?;
            let track = client.track(&id).await.context("failed to fetch track")?;
            print_entity(&CatalogEntity::Track(track), template.as_ref())
        }
        ListCommand::Video(options) => {
            let template = compile_optional_template(options.template)?;
            let id = list_resource_id(&options.id)?;
            let video = client.video(&id).await.context("failed to fetch video")?;
            print_entity(&CatalogEntity::Video(video), template.as_ref())
        }
        ListCommand::Album(options) => {
            let template = compile_optional_template(options.template)?;
            let id = list_resource_id(&options.id)?;
            let album = client.album(&id).await.context("failed to fetch album")?;
            print_entity(&CatalogEntity::Album(album), template.as_ref())
        }
        ListCommand::Playlist(options) => {
            let template = compile_optional_template(options.template)?;
            let id = list_resource_id(&options.id)?;
            let playlist = client
                .playlist(&id)
                .await
                .context("failed to fetch playlist")?;
            print_entity(&CatalogEntity::Playlist(playlist), template.as_ref())
        }
        ListCommand::Search { query, template } => {
            let template = compile_optional_template(template)?;
            let results = client
                .search(&query)
                .await
                .context("failed to search TIDAL")?;
            if let Some(top_hit) = results.top_hit {
                print_grouped_entity("top_hit", &top_hit, template.as_ref())?;
            }
            for track in results.tracks {
                print_grouped_entity("track", &CatalogEntity::Track(track), template.as_ref())?;
            }
            for album in results.albums {
                print_grouped_entity("album", &CatalogEntity::Album(album), template.as_ref())?;
            }
            for playlist in results.playlists {
                print_grouped_entity(
                    "playlist",
                    &CatalogEntity::Playlist(playlist),
                    template.as_ref(),
                )?;
            }
            for artist in results.artists {
                print_grouped_entity("artist", &CatalogEntity::Artist(artist), template.as_ref())?;
            }
            for video in results.videos {
                print_grouped_entity("video", &CatalogEntity::Video(video), template.as_ref())?;
            }
            Ok(())
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn run_download(options: &SharedOptions, args: DownloadArgs) -> Result<DownloadSummary> {
    if args.urls.is_empty() {
        anyhow::bail!("at least one URL or resource reference is required");
    }
    let config = load_runtime_config(options)?;
    let effective =
        effective_download_options(&config.download, config.metadata, config.m3u, &args)
            .context("failed to build effective download options")?;
    prevalidate_template(&effective.template)?;
    tokio::fs::create_dir_all(&effective.root)
        .await
        .with_context(|| {
            format!(
                "failed to create download root {}",
                effective.root.display()
            )
        })?;

    let provider = TidalProvider::new(tidal_client(options).await?);
    let http = http_client()?;
    let cover_cache = Arc::new(tokio::sync::Mutex::new(CoverCacheState::default()));
    let alloc = ProgressIdAllocator::new();
    let render_opts = TuiRenderOpts {
        output_root: effective.root.clone(),
        show_failures: effective.skip_errors,
        color: args.color,
    };
    let mut m3u_entries: Vec<(usize, M3uEntry)> = Vec::new();
    let mut summary = DownloadSummary::default();
    let mut next_index = 0usize;

    for input in &args.urls {
        let Some(reference) = provider
            .parse_ref(input)
            .await
            .with_context(|| format!("failed to parse input `{input}`"))?
        else {
            anyhow::bail!("unsupported TIDAL input: {input}");
        };
        let entity = provider
            .resolve(&reference)
            .await
            .with_context(|| format!("failed to resolve `{input}`"))?;

        let (op_kind, op_label) = operation_kind_and_label(&entity, input);
        let expansion = expand_download_entities(
            &provider,
            &reference,
            entity,
            effective.singles_mode,
            effective.skip_errors,
        )
        .await?;
        expansion.assert_valid_shape();
        if expansion.total_items() == 0 {
            summary.merge(expansion.summary);
            continue;
        }
        let ExpansionResult {
            items: expansion_items,
            groups: expansion_groups,
            summary: expansion_summary,
        } = expansion;
        summary.merge(expansion_summary);

        let op_id = alloc.next_operation_id();
        let mut scheduled = Vec::new();
        let mut group_tracker = GroupCompletionTracker::new();

        #[allow(clippy::if_not_else)]
        if !expansion_groups.is_empty() {
            // ── Grouped path (artist) ───────────────────────────────────
            let total_groups = Some(expansion_groups.len() as u64);
            let total_items: u64 = expansion_groups.iter().map(|g| g.items.len() as u64).sum();
            let op_config = OperationConfig {
                id: op_id,
                kind: op_kind,
                label: op_label,
                total_groups,
                total_items: Some(total_items),
            };
            let sink: Arc<dyn ProgressSink> =
                create_progress_sink(op_config, args.progress, render_opts.clone());

            for group in &expansion_groups {
                let group_id = alloc.next_group_id();
                group_tracker.register(group_id, group.items.len());
                sink.emit(DownloadEvent::GroupStarted {
                    operation_id: op_id,
                    group_id,
                    kind: OperationKind::Album,
                    label: group.label.clone(),
                    total_items: Some(group.items.len() as u64),
                });
                for entity in &group.items {
                    let item_id = alloc.next_item_id();
                    let (item_kind, item_label) = item_kind_and_label(entity);
                    sink.emit(DownloadEvent::ItemQueued {
                        operation_id: op_id,
                        group_id: Some(group_id),
                        item_id,
                        kind: item_kind,
                        label: item_label.clone(),
                        quality: None,
                    });
                    let Some(item_ref) = media_ref_for_entity(entity).cloned() else {
                        continue;
                    };
                    scheduled.push(ScheduledDownload {
                        index: next_index,
                        entity: entity.clone(),
                        item_ref,
                        parent_album: Some(group.album.clone()),
                        item_id,
                        operation_id: op_id,
                        group_id: Some(group_id),
                        item_label,
                    });
                    next_index += 1;
                }
            }

            let operation_entries = run_scheduled_downloads(
                scheduled,
                &provider,
                &http,
                &effective,
                &cover_cache,
                &sink,
                &mut summary,
                &mut group_tracker,
            )
            .await?;
            m3u_entries.extend(operation_entries);
            sink.emit(DownloadEvent::OperationFinished { id: op_id });
        } else {
            // ── Flat path (album, playlist, track, mix) ─────────────────
            let op_config = OperationConfig {
                id: op_id,
                kind: op_kind,
                label: op_label,
                total_groups: None,
                total_items: Some(expansion_items.len() as u64),
            };
            let sink: Arc<dyn ProgressSink> =
                create_progress_sink(op_config, args.progress, render_opts.clone());

            for (entity, parent_album) in expansion_items {
                let item_id = alloc.next_item_id();
                let (item_kind, item_label) = item_kind_and_label(&entity);
                sink.emit(DownloadEvent::ItemQueued {
                    operation_id: op_id,
                    group_id: None,
                    item_id,
                    kind: item_kind,
                    label: item_label.clone(),
                    quality: None,
                });
                let Some(item_ref) = media_ref_for_entity(&entity).cloned() else {
                    continue;
                };
                scheduled.push(ScheduledDownload {
                    index: next_index,
                    entity,
                    item_ref,
                    parent_album,
                    item_id,
                    operation_id: op_id,
                    group_id: None,
                    item_label,
                });
                next_index += 1;
            }

            let operation_entries = run_scheduled_downloads(
                scheduled,
                &provider,
                &http,
                &effective,
                &cover_cache,
                &sink,
                &mut summary,
                &mut group_tracker,
            )
            .await?;
            m3u_entries.extend(operation_entries);
            sink.emit(DownloadEvent::OperationFinished { id: op_id });
        }
    }

    if effective.m3u.save && !m3u_entries.is_empty() {
        m3u_entries.sort_by_key(|(index, _)| *index);
        let m3u_entries = m3u_entries
            .into_iter()
            .map(|(_, entry)| entry)
            .collect::<Vec<_>>();
        let output = effective.root.join("riptide.m3u");
        tokio::task::spawn_blocking(move || write_m3u(&m3u_entries, &output))
            .await
            .context("failed to join M3U writer")?
            .context("failed to write M3U playlist")?;
    }
    Ok(summary)
}

fn create_progress_sink(
    config: OperationConfig,
    _mode: ProgressMode,
    opts: TuiRenderOpts,
) -> Arc<dyn ProgressSink> {
    Arc::new(TuiSink::new(config, opts))
}

#[allow(clippy::too_many_arguments)]
async fn run_scheduled_downloads(
    scheduled: Vec<ScheduledDownload>,
    provider: &TidalProvider,
    http: &HttpClient,
    options: &EffectiveDownloadOptions,
    cover_cache: &CoverCache,
    sink: &Arc<dyn ProgressSink>,
    summary: &mut DownloadSummary,
    group_tracker: &mut GroupCompletionTracker,
) -> Result<Vec<(usize, M3uEntry)>> {
    let mut pending = scheduled.into_iter();
    let mut in_flight = JoinSet::new();
    let mut entries = Vec::new();

    fill_download_queue(
        &mut in_flight,
        &mut pending,
        provider,
        http,
        options,
        cover_cache,
        sink,
    );

    while let Some(result) = in_flight.join_next().await {
        match result.context("download task panicked")? {
            Ok(output) => {
                match output.completion {
                    ItemCompletion::Downloaded(entry) => {
                        summary.downloaded += 1;
                        if let Some(entry) = entry {
                            entries.push((output.index, entry));
                        }
                    }
                    ItemCompletion::Skipped(entry) => {
                        summary.skipped += 1;
                        if let Some(entry) = entry {
                            entries.push((output.index, entry));
                        }
                    }
                }
                if let Some((gid, outcome)) =
                    group_tracker.record_item_finished(output.group_id, false)
                {
                    sink.emit(DownloadEvent::GroupFinished {
                        group_id: gid,
                        outcome,
                    });
                }
                fill_download_queue(
                    &mut in_flight,
                    &mut pending,
                    provider,
                    http,
                    options,
                    cover_cache,
                    sink,
                );
            }
            Err(failure) if options.skip_errors => {
                summary.failed_items += 1;
                let message = failure.message();
                summary.failure_messages.push(message.clone());
                sink.emit(DownloadEvent::ItemFinished {
                    item_id: failure.item_id,
                    outcome: ItemOutcome::Failed { message },
                });
                if let Some((gid, outcome)) =
                    group_tracker.record_item_finished(failure.group_id, true)
                {
                    sink.emit(DownloadEvent::GroupFinished {
                        group_id: gid,
                        outcome,
                    });
                }
                fill_download_queue(
                    &mut in_flight,
                    &mut pending,
                    provider,
                    http,
                    options,
                    cover_cache,
                    sink,
                );
            }
            Err(failure) => {
                in_flight.abort_all();
                return Err(failure.error).with_context(|| {
                    format!(
                        "{} ({}) failed",
                        display_label_text(&failure.item_label),
                        media_ref_label(&failure.item_ref)
                    )
                });
            }
        }
    }

    Ok(entries)
}

fn fill_download_queue(
    in_flight: &mut JoinSet<std::result::Result<DownloadTaskOutput, DownloadTaskFailure>>,
    pending: &mut std::vec::IntoIter<ScheduledDownload>,
    provider: &TidalProvider,
    http: &HttpClient,
    options: &EffectiveDownloadOptions,
    cover_cache: &CoverCache,
    sink: &Arc<dyn ProgressSink>,
) {
    while in_flight.len() < options.concurrency {
        let Some(scheduled) = pending.next() else {
            break;
        };
        let provider = provider.clone();
        let http = http.clone();
        let options = options.clone();
        let cover_cache = Arc::clone(cover_cache);
        let sink = Arc::clone(sink);
        let item_ref = scheduled.item_ref.clone();
        in_flight.spawn(async move {
            match download_entity(
                &provider,
                &http,
                &options,
                &cover_cache,
                scheduled.parent_album.as_ref(),
                &scheduled.entity,
                scheduled.item_id,
                scheduled.operation_id,
                &sink,
            )
            .await
            {
                Ok(completion) => Ok(DownloadTaskOutput {
                    index: scheduled.index,
                    group_id: scheduled.group_id,
                    completion,
                }),
                Err(error) => Err(DownloadTaskFailure {
                    item_id: scheduled.item_id,
                    group_id: scheduled.group_id,
                    item_label: scheduled.item_label,
                    item_ref,
                    error,
                }),
            }
        });
    }
}

const fn media_ref_for_entity(entity: &CatalogEntity) -> Option<&MediaRef> {
    match entity {
        CatalogEntity::Track(track) => Some(&track.id),
        CatalogEntity::Album(album) => Some(&album.id),
        CatalogEntity::Playlist(playlist) => Some(&playlist.id),
        CatalogEntity::Artist(artist) => artist.id.as_ref(),
        CatalogEntity::Collection(collection) => Some(&collection.id),
        _ => None,
    }
}

fn media_ref_label(reference: &MediaRef) -> String {
    format!(
        "{}:{}:{}",
        reference.provider.as_str(),
        resource_kind_label(reference.kind),
        reference.id.as_str()
    )
}

const fn resource_kind_label(kind: riptide_core::ResourceKind) -> &'static str {
    match kind {
        riptide_core::ResourceKind::Track => "track",
        riptide_core::ResourceKind::Video => "video",
        riptide_core::ResourceKind::Album => "album",
        riptide_core::ResourceKind::Playlist => "playlist",
        riptide_core::ResourceKind::Artist => "artist",
        riptide_core::ResourceKind::Mix => "mix",
        _ => "unknown",
    }
}

fn display_label_text(label: &DisplayLabel) -> String {
    match (&label.artist, &label.group, label.track_number) {
        (Some(artist), Some(group), Some(number)) => {
            format!("{artist} - {group} - {:02} {}", number.get(), label.title)
        }
        (Some(artist), Some(group), None) => format!("{artist} - {group} - {}", label.title),
        (Some(artist), None, Some(number)) => {
            format!("{artist} - {:02} {}", number.get(), label.title)
        }
        (Some(artist), None, None) => format!("{artist} - {}", label.title),
        (None, Some(group), Some(number)) => {
            format!("{group} - {:02} {}", number.get(), label.title)
        }
        (None, Some(group), None) => format!("{group} - {}", label.title),
        (None, None, Some(number)) => format!("{:02} {}", number.get(), label.title),
        (None, None, None) => label.title.clone(),
    }
}

fn print_failure_messages(messages: &[String], limit: usize) {
    for message in messages.iter().take(limit) {
        eprintln!("failed: {message}");
    }
    if messages.len() > limit {
        eprintln!("... {} more failure(s)", messages.len() - limit);
    }
}

fn operation_kind_and_label(
    entity: &CatalogEntity,
    fallback_input: &str,
) -> (OperationKind, DisplayLabel) {
    match entity {
        CatalogEntity::Track(t) => (OperationKind::Track, label_for_track(t)),
        CatalogEntity::Album(a) => (OperationKind::Album, label_for_album(a)),
        CatalogEntity::Playlist(p) => (
            OperationKind::Playlist,
            DisplayLabel {
                title: p.title.clone(),
                artist: p.owner.clone(),
                group: None,
                track_number: None,
            },
        ),
        CatalogEntity::Artist(a) => (
            OperationKind::Artist,
            DisplayLabel {
                title: a.name.clone(),
                artist: None,
                group: None,
                track_number: None,
            },
        ),
        CatalogEntity::Collection(collection) => {
            let kind = if collection.kind == CollectionKind::Mix {
                OperationKind::Mix
            } else {
                OperationKind::Collection
            };
            (
                kind,
                DisplayLabel {
                    title: collection
                        .title
                        .clone()
                        .unwrap_or_else(|| fallback_input.to_owned()),
                    artist: collection.owner.clone(),
                    group: None,
                    track_number: None,
                },
            )
        }
        _ => (
            OperationKind::Collection,
            DisplayLabel {
                title: fallback_input.to_owned(),
                artist: None,
                group: None,
                track_number: None,
            },
        ),
    }
}

fn item_kind_and_label(entity: &CatalogEntity) -> (ItemKind, DisplayLabel) {
    match entity {
        CatalogEntity::Track(t) => (ItemKind::Track, label_for_track(t)),
        // Non-leaf entities should not appear in item lists, but handle gracefully.
        CatalogEntity::Album(a) => (
            ItemKind::Track,
            DisplayLabel {
                title: a.title.clone(),
                artist: a.artists.first().map(|c| c.artist.name.clone()),
                group: None,
                track_number: None,
            },
        ),
        other => (
            ItemKind::Track,
            DisplayLabel {
                title: format!("{other:?}"),
                artist: None,
                group: None,
                track_number: None,
            },
        ),
    }
}

async fn expand_download_entities(
    provider: &(dyn CollectionProvider + Send + Sync),
    reference: &MediaRef,
    entity: CatalogEntity,
    singles_mode: SinglesMode,
    skip_errors: bool,
) -> Result<ExpansionResult> {
    let items = match entity {
        CatalogEntity::Album(album) => provider
            .children(reference)
            .await
            .context("failed to fetch album children")?
            .into_iter()
            .filter_map(|entity| match entity {
                CatalogEntity::Track(_) => Some((entity, Some(album.clone()))),
                _ => None,
            })
            .collect(),
        CatalogEntity::Playlist(_) => provider
            .children(reference)
            .await
            .context("failed to fetch playlist children")?
            .into_iter()
            .filter_map(track_without_album)
            .collect(),
        CatalogEntity::Track(_) => vec![(entity, None)],
        CatalogEntity::Artist(_) => {
            return expand_artist_download_entities(provider, reference, singles_mode, skip_errors)
                .await;
        }
        CatalogEntity::Collection(_) => provider
            .children(reference)
            .await
            .context("failed to fetch collection children")?
            .into_iter()
            .filter_map(track_without_album)
            .collect(),
        _ => Vec::new(),
    };
    Ok(ExpansionResult {
        items,
        groups: Vec::new(),
        summary: DownloadSummary::default(),
    })
}

async fn expand_artist_download_entities(
    provider: &(dyn CollectionProvider + Send + Sync),
    reference: &MediaRef,
    singles_mode: SinglesMode,
    skip_errors: bool,
) -> Result<ExpansionResult> {
    let albums: Vec<Album> = provider
        .children(reference)
        .await
        .context("failed to fetch artist albums")?
        .into_iter()
        .filter_map(|e| match e {
            CatalogEntity::Album(a) => Some(a),
            _ => None,
        })
        .filter(|a| !should_skip_by_singles_mode(a, singles_mode))
        .collect();
    let mut result = ExpansionResult::default();
    for album in albums {
        let children = match provider.children(&album.id).await {
            Ok(children) => children,
            Err(error) if skip_errors => {
                result.summary.failed_expansions += 1;
                result.summary.failure_messages.push(format!(
                    "failed to fetch album items for {} ({}): {error:#}",
                    album.title,
                    media_ref_label(&album.id)
                ));
                continue;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to fetch album items for {} ({})",
                        album.title,
                        media_ref_label(&album.id)
                    )
                });
            }
        };
        let tracks: Vec<CatalogEntity> = children
            .into_iter()
            .filter(|e| matches!(e, CatalogEntity::Track(_)))
            .collect();
        if tracks.is_empty() {
            continue;
        }
        result.groups.push(ExpandedGroup {
            label: label_for_album(&album),
            album,
            items: tracks,
        });
    }
    Ok(result)
}

fn track_without_album(entity: CatalogEntity) -> Option<(CatalogEntity, Option<Album>)> {
    match entity {
        CatalogEntity::Track(_) => Some((entity, None)),
        _ => None,
    }
}

fn should_skip_by_singles_mode(album: &Album, singles_mode: SinglesMode) -> bool {
    let album_type = AlbumType::from_release_type(album.release_type.as_deref());
    match singles_mode {
        SinglesMode::Include => false,
        SinglesMode::Exclude => album_type.is_single_or_ep(),
        SinglesMode::Only => !album_type.is_single_or_ep(),
    }
}

#[allow(clippy::too_many_arguments)]
async fn download_entity(
    provider: &TidalProvider,
    http: &HttpClient,
    options: &EffectiveDownloadOptions,
    cover_cache: &CoverCache,
    parent_album: Option<&Album>,
    entity: &CatalogEntity,
    item_id: ItemId,
    operation_id: OperationId,
    sink: &Arc<dyn ProgressSink>,
) -> Result<ItemCompletion> {
    let Some(item) = download_item_for_entity(entity, options) else {
        return Ok(ItemCompletion::Skipped(None));
    };

    sink.emit(DownloadEvent::ItemPhaseChanged {
        item_id,
        phase: DownloadPhase::ResolvingMetadata,
    });
    let album_context = album_context_for_item(provider, parent_album, &item).await?;
    let context = context_for_download_item(options, &item, album_context.as_ref())?;

    sink.emit(DownloadEvent::ItemPhaseChanged {
        item_id,
        phase: DownloadPhase::ResolvingStream,
    });
    let stream = provider
        .streams(item.reference, item.quality.clone())
        .await
        .context("failed to resolve stream")?
        .into_iter()
        .next()
        .context("provider returned no stream variants")?;

    if let Some(badge) = quality_badge_for_stream(&stream) {
        sink.emit(DownloadEvent::ItemQualityResolved {
            item_id,
            quality: badge,
        });
    }

    let plan = render_download_plan(options, &context, &stream)?;
    let final_path = plan.final_path();

    sink.emit(DownloadEvent::ItemOutputResolved {
        item_id,
        path: final_path.clone(),
    });

    if options.skip_existing && final_path.exists() {
        sink.emit(DownloadEvent::ItemFinished {
            item_id,
            outcome: ItemOutcome::Exists,
        });
        return Ok(ItemCompletion::Skipped(Some(m3u_entry(
            &options.root,
            &final_path,
            item.title,
            item.duration,
        ))));
    }

    let prepared = execute_media_download(http, &plan, &final_path, item_id, sink).await?;

    sink.emit(DownloadEvent::ItemPhaseChanged {
        item_id,
        phase: DownloadPhase::Tagging,
    });
    let cover_data = write_metadata_if_enabled(
        provider,
        http,
        options.metadata,
        cover_cache,
        MetadataWriteInput {
            item: &item,
            album_context: album_context.as_ref(),
            processed: &prepared.processed,
            final_extension: plan.final_extension,
        },
    )
    .await?;

    sink.emit(DownloadEvent::ItemPhaseChanged {
        item_id,
        phase: DownloadPhase::Persisting,
    });
    persist_processed_file(&prepared.processed, &final_path).await?;

    // Lyrics sidecar
    write_lyrics_sidecar_with_events(
        provider,
        options,
        &item,
        &final_path,
        item_id,
        operation_id,
        sink,
    )
    .await?;

    // Cover sidecar
    write_cover_sidecar_with_events(
        options,
        cover_cache,
        cover_data,
        &final_path,
        operation_id,
        sink,
    )
    .await?;

    sink.emit(DownloadEvent::ItemFinished {
        item_id,
        outcome: ItemOutcome::Downloaded,
    });
    Ok(ItemCompletion::Downloaded(Some(m3u_entry(
        &options.root,
        &final_path,
        item.title,
        item.duration,
    ))))
}

struct DownloadItem<'a> {
    reference: &'a MediaRef,
    quality: QualityRequest,
    duration: Option<MediaDuration>,
    metadata: MediaMetadataInput<'a>,
    title: String,
}

fn download_item_for_entity<'a>(
    entity: &'a CatalogEntity,
    options: &EffectiveDownloadOptions,
) -> Option<DownloadItem<'a>> {
    match entity {
        CatalogEntity::Track(track) => Some(DownloadItem {
            reference: &track.id,
            quality: QualityRequest::Audio(options.audio_quality),
            duration: track.duration,
            metadata: MediaMetadataInput::Track(track),
            title: track.title.clone(),
        }),
        _ => None,
    }
}

async fn album_context_for_item(
    provider: &TidalProvider,
    parent_album: Option<&Album>,
    item: &DownloadItem<'_>,
) -> Result<Option<Album>> {
    if let Some(album) = parent_album {
        return Ok(Some(album.clone()));
    }

    let album_ref = match item.metadata {
        MediaMetadataInput::Track(track) => track.album.as_ref().map(|album| &album.id),
    };
    let Some(album_ref) = album_ref else {
        return Ok(None);
    };

    match provider
        .resolve(album_ref)
        .await
        .with_context(|| format!("failed to resolve album context `{album_ref:?}`"))?
    {
        CatalogEntity::Album(album) => Ok(Some(album)),
        other => anyhow::bail!(
            "album context reference `{album_ref:?}` resolved to {}",
            entity_kind(&other)
        ),
    }
}

fn context_for_download_item(
    options: &EffectiveDownloadOptions,
    item: &DownloadItem<'_>,
    album: Option<&Album>,
) -> Result<TemplateContext> {
    let now = time::OffsetDateTime::now_utc();
    let mut context = match item.metadata {
        MediaMetadataInput::Track(track) => TemplateContext::new(now).with_track(track),
    };
    if let Some(album) = album {
        context = context.with_album(album);
    }
    context = context.with_effective_quality_label(effective_quality_label(options.audio_quality)?);
    Ok(context)
}

fn effective_quality_label(
    audio_quality: AudioQualityRequest,
) -> Result<Option<ProviderQualityLabel>> {
    match audio_quality {
        AudioQualityRequest::HiResLossless | AudioQualityRequest::Best => {
            ProviderQualityLabel::new("MAX")
                .map(Some)
                .context("failed to build effective quality label")
        }
        AudioQualityRequest::Low | AudioQualityRequest::High | AudioQualityRequest::Lossless => {
            Ok(None)
        }
    }
}

fn cover_for_item(item: &DownloadItem<'_>, album: Option<&Album>) -> Option<CoverArtId> {
    album
        .and_then(|album| album.cover.clone())
        .or_else(|| match item.metadata {
            MediaMetadataInput::Track(track) => {
                track.album.as_ref().and_then(|album| album.cover.clone())
            }
        })
}

fn render_download_plan(
    options: &EffectiveDownloadOptions,
    context: &TemplateContext,
    stream: &riptide_core::StreamVariant,
) -> Result<riptide_media::DownloadPlan> {
    let rendered = options
        .template
        .render_with_options(
            context,
            TemplateRenderOptions {
                with_asterisk_ext: false,
            },
        )
        .context("failed to render output template")?;
    let output_stem = options.root.join(rendered.as_str());
    Ok(plan_download(stream, output_stem))
}

struct PreparedDownload {
    processed: PathBuf,
    _temp_dir: tempfile::TempDir,
}

async fn execute_media_download(
    http: &HttpClient,
    plan: &riptide_media::DownloadPlan,
    final_path: &Path,
    item_id: ItemId,
    sink: &Arc<dyn ProgressSink>,
) -> Result<PreparedDownload> {
    let final_parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(final_parent)
        .await
        .with_context(|| format!("failed to create {}", final_parent.display()))?;
    let temp_dir = tempfile::Builder::new()
        .prefix(".riptide-tmp-")
        .tempdir_in(final_parent)
        .with_context(|| format!("failed to create temp dir in {}", final_parent.display()))?;
    let download_path = temp_dir
        .path()
        .join(format!("download{}", plan.download_extension.as_str()));

    sink.emit(DownloadEvent::ItemPhaseChanged {
        item_id,
        phase: DownloadPhase::ResolvingMedia,
    });
    let resolved = resolve_manifest_async(&plan.manifest, http)
        .await
        .context("failed to resolve media manifest")?;

    sink.emit(DownloadEvent::ItemPhaseChanged {
        item_id,
        phase: DownloadPhase::Downloading,
    });
    let transfer = MediaTransferProgress::new(item_id, Arc::clone(sink));
    download_segments(
        http,
        &resolved.urls,
        &download_path,
        DownloadOptions::default(),
        Some(&transfer),
    )
    .await
    .context("failed to download media segments")?;

    sink.emit(DownloadEvent::ItemPhaseChanged {
        item_id,
        phase: DownloadPhase::PostProcessing,
    });
    let processed = post_process(download_path, &plan.post_process)
        .await
        .context("failed to post-process media")?;
    Ok(PreparedDownload {
        processed,
        _temp_dir: temp_dir,
    })
}

async fn write_metadata_if_enabled(
    provider: &TidalProvider,
    http: &HttpClient,
    metadata_config: MetadataConfig,
    cover_cache: &CoverCache,
    input: MetadataWriteInput<'_>,
) -> Result<Option<Arc<[u8]>>> {
    let cover = cover_for_item(input.item, input.album_context);
    let cover_data = if metadata_config.enable && metadata_config.cover {
        fetch_cover_data(http, cover_cache, cover.as_ref()).await?
    } else {
        None
    };
    if metadata_config.enable {
        let mut metadata = match input.item.metadata {
            MediaMetadataInput::Track(track) => metadata_from_track(track, input.album_context),
        };
        metadata.cover_data.clone_from(&cover_data);
        if metadata_config.lyrics && matches!(input.item.metadata, MediaMetadataInput::Track(_)) {
            metadata.lyrics = provider
                .lyrics(input.item.reference)
                .await
                .ok()
                .flatten()
                .and_then(|lyrics| lyrics.synchronized_lrc.or(lyrics.text));
        }
        let tag_path = input.processed.to_path_buf();
        let final_extension = input.final_extension;
        tokio::task::spawn_blocking(move || write_tags(&tag_path, final_extension, &metadata))
            .await
            .context("failed to join metadata writer")?
            .context("failed to write metadata")?;
    }
    Ok(cover_data)
}

async fn fetch_cover_data(
    http: &HttpClient,
    cover_cache: &CoverCache,
    cover: Option<&CoverArtId>,
) -> Result<Option<Arc<[u8]>>> {
    let Some(cover) = cover else {
        return Ok(None);
    };
    let cached = {
        let cache = cover_cache.lock().await;
        cache.bytes.get(cover).cloned()
    };
    if let Some(data) = cached {
        return Ok(Some(data));
    }
    let url = cover_url(cover, CoverArtSize::default()).context("failed to build cover URL")?;
    let response = http
        .get(url.clone())
        .send()
        .await
        .with_context(|| format!("failed to fetch cover art {url}"))?
        .error_for_status()
        .with_context(|| format!("failed to fetch cover art {url}"))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read cover art {url}"))?;
    let data = Arc::from(bytes.as_ref());
    cover_cache
        .lock()
        .await
        .bytes
        .insert(cover.clone(), Arc::clone(&data));
    Ok(Some(data))
}

async fn persist_processed_file(processed: &Path, final_path: &Path) -> Result<()> {
    if let Some(parent) = final_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    tokio::fs::rename(&processed, &final_path)
        .await
        .with_context(|| format!("failed to persist {}", final_path.display()))?;
    Ok(())
}

async fn write_lyrics_sidecar_with_events(
    provider: &TidalProvider,
    options: &EffectiveDownloadOptions,
    item: &DownloadItem<'_>,
    final_path: &Path,
    item_id: ItemId,
    operation_id: OperationId,
    sink: &Arc<dyn ProgressSink>,
) -> Result<()> {
    if !options.metadata.enable
        || !options.metadata.lyrics
        || !matches!(item.metadata, MediaMetadataInput::Track(_))
    {
        return Ok(());
    }
    let Ok(Some(lyrics)) = provider.lyrics(item.reference).await else {
        return Ok(());
    };
    let Some(lrc) = lyrics.synchronized_lrc else {
        return Ok(());
    };

    let sidecar_name = lyrics_sidecar_name(final_path);
    sink.emit(DownloadEvent::ArtifactPhaseChanged {
        operation_id,
        group_id: None,
        item_id: Some(item_id),
        kind: ArtifactKind::Lyrics,
        label: sidecar_name.clone(),
        phase: DownloadPhase::WritingLyrics,
    });
    let sidecar_path = final_path.to_path_buf();
    let result = tokio::task::spawn_blocking(move || write_lyrics_sidecar(&sidecar_path, &lrc))
        .await
        .context("failed to join lyrics sidecar writer")?
        .context("failed to write lyrics sidecar");
    let outcome = match &result {
        Ok(()) => ArtifactOutcome::Written,
        Err(e) => ArtifactOutcome::Failed {
            message: e.to_string(),
        },
    };
    sink.emit(DownloadEvent::ArtifactFinished {
        operation_id,
        group_id: None,
        item_id: Some(item_id),
        kind: ArtifactKind::Lyrics,
        label: sidecar_name,
        outcome,
    });
    result
}

async fn write_cover_sidecar_with_events(
    options: &EffectiveDownloadOptions,
    cover_cache: &CoverCache,
    cover_data: Option<Arc<[u8]>>,
    final_path: &Path,
    operation_id: OperationId,
    sink: &Arc<dyn ProgressSink>,
) -> Result<()> {
    if !options.metadata.enable || !options.metadata.cover {
        return Ok(());
    }
    let Some(cover_data) = cover_data else {
        return Ok(());
    };
    let cover_path = cover_sidecar_path(final_path);
    if !claim_cover_sidecar(cover_cache, &cover_path).await {
        return Ok(());
    }

    let label = cover_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("cover.jpg")
        .to_owned();
    sink.emit(DownloadEvent::ArtifactPhaseChanged {
        operation_id,
        group_id: None,
        item_id: None,
        kind: ArtifactKind::Cover,
        label: label.clone(),
        phase: DownloadPhase::WritingCover,
    });
    let result = tokio::task::spawn_blocking({
        let cover_path = cover_path.clone();
        move || save_cover(cover_data.as_ref(), &cover_path)
    })
    .await
    .context("failed to join cover writer")?
    .context("failed to write cover sidecar");

    if result.is_err() {
        cover_cache
            .lock()
            .await
            .written_sidecars
            .remove(&cover_path);
    }
    let outcome = match &result {
        Ok(()) => ArtifactOutcome::Written,
        Err(e) => ArtifactOutcome::Failed {
            message: e.to_string(),
        },
    };
    sink.emit(DownloadEvent::ArtifactFinished {
        operation_id,
        group_id: None,
        item_id: None,
        kind: ArtifactKind::Cover,
        label,
        outcome,
    });
    result
}

fn lyrics_sidecar_name(final_path: &Path) -> String {
    final_path
        .file_stem()
        .and_then(|s| s.to_str())
        .map_or_else(|| "lyrics.lrc".to_owned(), |stem| format!("{stem}.lrc"))
}

fn cover_sidecar_path(final_path: &Path) -> PathBuf {
    final_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cover.jpg")
}

enum MediaMetadataInput<'a> {
    Track(&'a Track),
}

struct MetadataWriteInput<'a> {
    item: &'a DownloadItem<'a>,
    album_context: Option<&'a Album>,
    processed: &'a Path,
    final_extension: riptide_core::ContainerExtension,
}

#[derive(Default)]
struct CoverCacheState {
    bytes: HashMap<CoverArtId, Arc<[u8]>>,
    written_sidecars: HashSet<PathBuf>,
}

type CoverCache = Arc<tokio::sync::Mutex<CoverCacheState>>;

async fn claim_cover_sidecar(cover_cache: &CoverCache, cover_path: &Path) -> bool {
    if cover_path.exists() {
        return false;
    }
    let mut cache = cover_cache.lock().await;
    cache.written_sidecars.insert(cover_path.to_path_buf())
}

fn m3u_entry(
    root: &Path,
    final_path: &Path,
    title: String,
    duration: Option<MediaDuration>,
) -> M3uEntry {
    let path = final_path
        .strip_prefix(root)
        .map_or_else(|_| final_path.to_path_buf(), Path::to_path_buf);
    M3uEntry {
        path,
        title,
        duration,
    }
}

fn compile_optional_template(template: Option<String>) -> Result<Option<CompiledTemplate>> {
    template
        .map(|template| CompiledTemplate::parse(&template).context("failed to compile template"))
        .transpose()
}

fn list_resource_id(value: &str) -> Result<ResourceId> {
    ResourceId::new(value).with_context(|| format!("invalid TIDAL resource ID `{value}`"))
}

fn prevalidate_template(template: &CompiledTemplate) -> Result<()> {
    template
        .render_with_options(
            &TemplateContext::synthetic(),
            TemplateRenderOptions {
                with_asterisk_ext: false,
            },
        )
        .context("template validation failed")?;
    Ok(())
}

fn load_runtime_config(options: &SharedOptions) -> Result<ValidatedConfig> {
    let Some(path) = options.config_file.clone().or_else(default_config_file) else {
        return Ok(ValidatedConfig::default());
    };
    if !path.exists() {
        return Ok(ValidatedConfig::default());
    }
    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to load config from {}", path.display()))?;
    ValidatedConfig::from_toml_str(&contents)
        .with_context(|| format!("failed to validate config from {}", path.display()))
}

fn default_config_file() -> Option<PathBuf> {
    project_dirs().map(|dirs| dirs.config_dir().join("config.toml"))
}

fn effective_download_options(
    download: &DownloadConfig,
    metadata: MetadataConfig,
    m3u: M3uConfig,
    args: &DownloadArgs,
) -> Result<EffectiveDownloadOptions> {
    let audio_quality = parse_or_config_audio(args.audio_quality.as_deref(), download)?;
    let concurrency = args.concurrency.map_or_else(
        || Ok(download.concurrency.get()),
        |value| Concurrency::new(value).map(Concurrency::get),
    )?;
    let root = args
        .path
        .clone()
        .or_else(|| {
            download
                .download_root
                .as_ref()
                .map(|root| root.as_path().to_path_buf())
        })
        .or_else(default_download_root)
        .context("failed to determine download root")?;
    let template = match &args.output {
        Some(template) => {
            CompiledTemplate::parse(template).context("failed to compile output template")?
        }
        None => download.templates.default.clone(),
    };
    let skip_existing = if args.existing.no_skip_existing {
        false
    } else if args.existing.skip_existing {
        true
    } else {
        download.skip_existing
    };
    Ok(EffectiveDownloadOptions {
        root,
        audio_quality,
        skip_existing,
        concurrency: usize::from(concurrency),
        template,
        metadata: MetadataConfig {
            enable: metadata.enable && !args.metadata.no_metadata,
            lyrics: metadata.lyrics && !args.metadata.no_lyrics,
            cover: metadata.cover && !args.metadata.no_cover,
        },
        m3u,
        singles_mode: args
            .singles
            .map_or(download.singles_mode, SinglesMode::from),
        skip_errors: args.errors.skip_errors,
    })
}

fn parse_or_config_audio(
    value: Option<&str>,
    download: &DownloadConfig,
) -> Result<riptide_core::AudioQualityRequest> {
    value.map_or(Ok(download.audio_quality), |value| {
        value
            .parse()
            .with_context(|| format!("invalid audio quality `{value}`"))
    })
}

fn default_download_root() -> Option<PathBuf> {
    UserDirs::new().and_then(|dirs| {
        dirs.audio_dir()
            .map(Path::to_path_buf)
            .or_else(|| Some(dirs.home_dir().join("Music")))
    })
}

fn print_grouped_entity(
    group: &str,
    entity: &CatalogEntity,
    template: Option<&CompiledTemplate>,
) -> Result<()> {
    if template.is_some() {
        return print_entity(entity, template);
    }
    println!("{group}\t{}", entity_row(entity));
    Ok(())
}

fn print_entity(entity: &CatalogEntity, template: Option<&CompiledTemplate>) -> Result<()> {
    if let Some(template) = template {
        let context = context_for_entity(entity);
        let rendered = template
            .render(&context)
            .context("failed to render template")?;
        println!("{}", rendered.as_str());
    } else {
        println!("{}", entity_row(entity));
    }
    Ok(())
}

fn context_for_entity(entity: &CatalogEntity) -> TemplateContext {
    let now = time::OffsetDateTime::now_utc();
    match entity {
        CatalogEntity::Track(track) => TemplateContext::new(now).with_track(track),
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
        _ => TemplateContext::new(now),
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
            artist.id.as_ref().map_or("", |id| id.id.as_str()),
            artist.name
        ),
        CatalogEntity::Collection(collection) => format!(
            "collection\t{}\t{}",
            collection.id.id.as_str(),
            collection.title.as_deref().unwrap_or("")
        ),
        _ => "unknown\t\t".to_owned(),
    }
}

const fn entity_kind(entity: &CatalogEntity) -> &'static str {
    match entity {
        CatalogEntity::Track(_) => "track",
        CatalogEntity::Video(_) => "video",
        CatalogEntity::Album(_) => "album",
        CatalogEntity::Playlist(_) => "playlist",
        CatalogEntity::Artist(_) => "artist",
        CatalogEntity::Collection(_) => "collection",
        _ => "unknown",
    }
}

fn http_client() -> Result<HttpClient> {
    riptide_provider_tidal::http_client().context("failed to build HTTP client")
}

async fn tidal_client(options: &SharedOptions) -> Result<TidalClient> {
    let http = http_client()?;
    let auth = auth_client(http.clone(), options)?;
    let token_store = Arc::new(FileTokenStore::new(auth_file(options)?));
    let cache = Arc::new(
        RedbApiCache::open(cache_file(options)?)
            .await
            .context("failed to open TIDAL cache")?,
    );
    Ok(TidalClient::new(
        http,
        options.api_base_url.clone(),
        auth,
        token_store,
        cache,
    ))
}

fn auth_client(http: HttpClient, options: &SharedOptions) -> Result<TidalAuthClient> {
    let credentials = TidalCredentials::from_env().context("failed to load TIDAL credentials")?;
    Ok(TidalAuthClient::new(
        http,
        options.auth_base_url.clone(),
        credentials,
    ))
}

fn auth_file(options: &SharedOptions) -> Result<PathBuf> {
    options
        .auth_file
        .clone()
        .or_else(|| project_dirs().map(|dirs| dirs.data_local_dir().join("tidal-token.json")))
        .context("failed to determine auth file path")
}

fn cache_file(options: &SharedOptions) -> Result<PathBuf> {
    options
        .cache_file
        .clone()
        .or_else(|| project_dirs().map(|dirs| dirs.cache_dir().join("tidal-cache.redb")))
        .context("failed to determine cache file path")
}

fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("dev", "riptide", "riptide-rs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use riptide_core::{
        AlbumSummary, Artist, ArtistCredit, ArtistRole, ExternalIds, ProviderId, ResourceId,
        ResourceKind,
    };

    #[test]
    fn download_context_uses_resolved_album_for_date_template() -> Result<()> {
        let entity = CatalogEntity::Track(sample_track()?);
        let album = sample_album()?;
        let options = test_download_options("{album.date:%Y} - {item.title}")?;
        let item = download_item_for_entity(&entity, &options).context("expected download item")?;

        let context = context_for_download_item(&options, &item, Some(&album))?;
        let rendered = options.template.render_with_options(
            &context,
            TemplateRenderOptions {
                with_asterisk_ext: false,
            },
        )?;

        assert_eq!(rendered.as_str(), "2023 - Track Title");
        Ok(())
    }

    #[test]
    fn cover_sidecar_uses_album_directory_cover_name() {
        let final_path = Path::new("/music/Artist/2023 - Album/01 Track.flac");
        assert_eq!(
            cover_sidecar_path(final_path),
            PathBuf::from("/music/Artist/2023 - Album/cover.jpg")
        );
    }

    #[tokio::test]
    async fn cover_sidecar_claims_path_once() {
        let cache = Arc::new(tokio::sync::Mutex::new(CoverCacheState::default()));
        let path = Path::new("/music/Artist/Album/cover.jpg");

        assert!(claim_cover_sidecar(&cache, path).await);
        assert!(!claim_cover_sidecar(&cache, path).await);
    }

    #[test]
    fn album_type_classifies_release_types_for_singles_filtering() {
        assert_eq!(
            AlbumType::from_release_type(Some("ALBUM")),
            AlbumType::Album
        );
        assert_eq!(
            AlbumType::from_release_type(Some("COMPILATION")),
            AlbumType::Album
        );
        assert_eq!(
            AlbumType::from_release_type(Some("SINGLE")),
            AlbumType::Single
        );
        assert_eq!(AlbumType::from_release_type(Some("EP")), AlbumType::Ep);
        assert_eq!(AlbumType::from_release_type(None), AlbumType::Unknown);
        assert!(!AlbumType::Unknown.is_single_or_ep());
    }

    #[test]
    fn template_prevalidation_uses_synthetic_context() -> Result<()> {
        let template = CompiledTemplate::parse(
            "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}",
        )?;

        prevalidate_template(&template)
    }

    fn test_download_options(template: &str) -> Result<EffectiveDownloadOptions> {
        Ok(EffectiveDownloadOptions {
            root: PathBuf::from("/tmp/riptide-test"),
            audio_quality: AudioQualityRequest::Lossless,
            skip_existing: true,
            concurrency: 1,
            template: CompiledTemplate::parse(template)?,
            metadata: MetadataConfig {
                enable: true,
                lyrics: false,
                cover: false,
            },
            m3u: M3uConfig { save: false },
            singles_mode: SinglesMode::Include,
            skip_errors: false,
        })
    }

    fn sample_track() -> Result<Track> {
        Ok(Track {
            id: media_ref(ResourceKind::Track, "track-1")?,
            title: "Track Title".to_owned(),
            artists: vec![artist_credit("Artist")?],
            album: Some(AlbumSummary {
                id: media_ref(ResourceKind::Album, "album-1")?,
                title: "Album Title".to_owned(),
                artists: vec![artist_credit("Artist")?],
                cover: None,
            }),
            duration: None,
            track_number: None,
            disc_number: None,
            explicit: None,
            external_ids: ExternalIds::default(),
            version: None,
            copyright: None,
            bpm: None,
            quality_label: None,
            dolby_atmos: false,
        })
    }

    fn sample_album() -> Result<Album> {
        Ok(Album {
            id: media_ref(ResourceKind::Album, "album-1")?,
            title: "Album Title".to_owned(),
            artists: vec![artist_credit("Artist")?],
            release_date: Some(time::Date::from_calendar_date(2023, time::Month::April, 1)?),
            track_count: None,
            explicit: None,
            release_type: None,
            hires_lossless: false,
            cover: None,
        })
    }

    fn artist_credit(name: &str) -> Result<ArtistCredit> {
        Ok(ArtistCredit {
            artist: Artist {
                id: Some(media_ref(ResourceKind::Artist, "artist-1")?),
                name: name.to_owned(),
            },
            role: ArtistRole::Main,
        })
    }

    fn media_ref(kind: ResourceKind, id: &str) -> Result<MediaRef> {
        Ok(MediaRef::new(
            ProviderId::new("tidal")?,
            kind,
            ResourceId::new(id)?,
        ))
    }

    #[test]
    fn group_tracker_completes_groups_out_of_order() {
        let alloc = ProgressIdAllocator::new();
        let g1 = alloc.next_group_id();
        let g2 = alloc.next_group_id();
        let mut tracker = GroupCompletionTracker::new();
        tracker.register(g1, 2);
        tracker.register(g2, 1);

        assert!(tracker.record_item_finished(Some(g1), false).is_none());
        assert_eq!(
            tracker.record_item_finished(Some(g2), false),
            Some((g2, GroupOutcome::Done))
        );
        assert_eq!(
            tracker.record_item_finished(Some(g1), false),
            Some((g1, GroupOutcome::Done))
        );
    }

    #[test]
    fn group_tracker_reports_partial_on_failure() {
        let alloc = ProgressIdAllocator::new();
        let g1 = alloc.next_group_id();
        let mut tracker = GroupCompletionTracker::new();
        tracker.register(g1, 2);

        assert!(tracker.record_item_finished(Some(g1), true).is_none());
        assert_eq!(
            tracker.record_item_finished(Some(g1), false),
            Some((g1, GroupOutcome::Partial { failed: 1 }))
        );
    }

    #[test]
    fn group_tracker_ignores_none_group_id() {
        let mut tracker = GroupCompletionTracker::new();
        assert!(tracker.record_item_finished(None, false).is_none());
    }
}
