import typer
from typing_extensions import Annotated

from riptide.cli.commands.auth import refresh
from riptide.cli.commands.subcommands import register_subcommands
from riptide.cli.config import (
    ARTIST_SINGLES_FILTER_LITERAL,
    CONFIG,
    VIDEOS_FILTER_LITERAL,
)
from riptide.cli.ctx import Context
from riptide.cli.utils.resource import TidalResource
from riptide.core.api.models import Album, Track, Video
from riptide.core.utils.format import generate_template_data

list_command = typer.Typer(name="list")
register_subcommands(list_command)


@list_command.callback(no_args_is_help=True)
def list_callback(
    ctx: Context,
    FORMAT: Annotated[
        str,
        typer.Option(
            "--output",
            "-o",
            help="Format output list template.",
        ),
    ] = "",
    SINGLES_FILTER: Annotated[
        ARTIST_SINGLES_FILTER_LITERAL,
        typer.Option(
            "--singles",
            "-s",
            help="Filter for including artists' singles, used while listing artist.",
        ),
    ] = CONFIG.download.singles_filter,
    VIDEOS_FILTER: Annotated[
        VIDEOS_FILTER_LITERAL,
        typer.Option(
            "--videos",
            "-vid",
            help="Videos handling: 'none' to exclude, 'allow' to include, 'only' to list videos only.",
        ),
    ] = CONFIG.download.videos_filter,
):
    """
    List information about Tidal resources.
    """

    ctx.invoke(refresh, EARLY_EXPIRE_TIME=600, SILENT=True)

    # Use provided format, or fall back to config, or fall back to default
    format_template = FORMAT or CONFIG.list.format

    def list_resources():
        def format_track_line(track: Track, album: Album | None = None) -> str:
            """Format track information using the template."""
            try:
                # Generate template data
                template_data = generate_template_data(
                    item=track,
                    album=album,
                    quality=track.audioQuality,
                )

                # Format using the template
                formatted = format_template.format(**template_data)
                return formatted
            except (KeyError, AttributeError) as e:
                # Fallback to basic format if template fails
                return f"Error formatting track {track.id}: {e}"

        def format_video_line(video: Video) -> str:
            """Format video information using the template (if videos are included)."""
            try:
                # Generate template data for video
                template_data = generate_template_data(
                    item=video,
                    quality=video.quality or "UNKNOWN",
                )

                # Format using the template
                formatted = format_template.format(**template_data)
                return formatted
            except (KeyError, AttributeError) as e:
                # Fallback to basic format if template fails
                return f"Error formatting video {video.id}: {e}"

        def list_track(resource: TidalResource):
            """List a single track."""
            track = ctx.obj.api.get_track(resource.id)
            album = ctx.obj.api.get_album(track.album.id)
            ctx.obj.console.print(format_track_line(track, album))

        def list_video(resource: TidalResource):
            """List a single video."""
            video = ctx.obj.api.get_video(resource.id)
            ctx.obj.console.print(format_video_line(video))

        def list_album(resource: TidalResource):
            """List all tracks in an album."""
            album = ctx.obj.api.get_album(resource.id)

            offset = 0
            while True:
                album_items = ctx.obj.api.get_album_items(
                    album_id=album.id, offset=offset
                )

                for album_item in album_items.items:
                    item = album_item.item

                    if isinstance(item, Video):
                        if VIDEOS_FILTER == "none":
                            continue
                        ctx.obj.console.print(format_video_line(item))
                    elif isinstance(item, Track):
                        if VIDEOS_FILTER == "only":
                            continue
                        ctx.obj.console.print(format_track_line(item, album))

                if len(album_items.items) < album_items.limit:
                    break

                offset += album_items.limit

        def list_playlist(resource: TidalResource):
            """List all tracks in a playlist."""
            playlist = ctx.obj.api.get_playlist(resource.id)

            offset = 0
            while True:
                playlist_items = ctx.obj.api.get_playlist_items(
                    playlist_uuid=playlist.uuid, offset=offset
                )

                for playlist_item in playlist_items.items:
                    item = playlist_item.item

                    if isinstance(item, Video):
                        if VIDEOS_FILTER == "none":
                            continue
                        ctx.obj.console.print(format_video_line(item))
                    elif isinstance(item, Track):
                        if VIDEOS_FILTER == "only":
                            continue
                        # Fetch full album info for better display
                        album = ctx.obj.api.get_album(item.album.id)
                        ctx.obj.console.print(format_track_line(item, album))

                if len(playlist_items.items) < playlist_items.limit:
                    break

                offset += playlist_items.limit

        def list_artist(resource: TidalResource):
            """List all tracks from an artist's albums."""
            artist = ctx.obj.api.get_artist(resource.id)

            # Helper to list all tracks from albums
            def list_albums_tracks(filter_type: str):
                offset = 0
                while True:
                    artist_albums = ctx.obj.api.get_artist_albums(
                        artist_id=artist.id,
                        filter=filter_type,
                        offset=offset,
                    )

                    for album in artist_albums.items:
                        # List all tracks in this album
                        album_offset = 0
                        while True:
                            album_items = ctx.obj.api.get_album_items(
                                album_id=album.id, offset=album_offset
                            )

                            for album_item in album_items.items:
                                item = album_item.item

                                if isinstance(item, Video):
                                    if VIDEOS_FILTER == "none":
                                        continue
                                    ctx.obj.console.print(format_video_line(item))
                                elif isinstance(item, Track):
                                    if VIDEOS_FILTER == "only":
                                        continue
                                    ctx.obj.console.print(
                                        format_track_line(item, album)
                                    )

                            if len(album_items.items) < album_items.limit:
                                break

                            album_offset += album_items.limit

                    if len(artist_albums.items) < artist_albums.limit:
                        break

                    offset += artist_albums.limit

            # List albums
            if VIDEOS_FILTER != "only":
                list_albums_tracks("ALBUMS")

                # List singles/EPs if requested
                if SINGLES_FILTER in ["only", "include"]:
                    list_albums_tracks("EPSANDSINGLES")

            # List videos if requested
            if VIDEOS_FILTER in ["allow", "only"]:
                offset = 0
                while True:
                    artist_videos = ctx.obj.api.get_artist_videos(
                        artist_id=artist.id,
                        offset=offset,
                    )

                    for video in artist_videos.items:
                        ctx.obj.console.print(format_video_line(video))

                    if len(artist_videos.items) < artist_videos.limit:
                        break

                    offset += artist_videos.limit

        def list_mix(resource: TidalResource):
            """List all tracks in a mix."""
            offset = 0
            while True:
                mix_items = ctx.obj.api.get_mix_items(mix_id=resource.id, offset=offset)

                for mix_item in mix_items.items:
                    item = mix_item.item

                    if isinstance(item, Video):
                        if VIDEOS_FILTER == "none":
                            continue
                        ctx.obj.console.print(format_video_line(item))
                    elif isinstance(item, Track):
                        if VIDEOS_FILTER == "only":
                            continue
                        # Fetch full album info for better display
                        album = ctx.obj.api.get_album(item.album.id)
                        ctx.obj.console.print(format_track_line(item, album))

                if len(mix_items.items) < mix_items.limit:
                    break

                offset += mix_items.limit

        # Process each resource
        for resource in ctx.obj.resources:
            try:
                if resource.type == "track":
                    list_track(resource)
                elif resource.type == "video":
                    list_video(resource)
                elif resource.type == "album":
                    list_album(resource)
                elif resource.type == "playlist":
                    list_playlist(resource)
                elif resource.type == "artist":
                    list_artist(resource)
                elif resource.type == "mix":
                    list_mix(resource)
            except Exception as e:
                ctx.obj.console.print(f"[red]Error processing {resource}:[/] {e}")
                continue

    ctx.call_on_close(list_resources)
