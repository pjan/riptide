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

list_command = typer.Typer(name="list")
register_subcommands(list_command)


@list_command.callback(no_args_is_help=True)
def list_callback(
    ctx: Context,
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

    ctx.invoke(refresh, EARLY_EXPIRE_TIME=600)

    def list_resources():
        def format_track_line(track: Track, album: Album | None = None) -> str:
            """Format track information as a single line."""
            # Basic info
            parts = [
                f"ID: {track.id}",
                f"Title: {track.full_name}",
            ]

            # Artist(s)
            if track.artist:
                parts.append(f"Artist: {track.artist.name}")
            elif track.artists:
                artist_names = ", ".join(a.name for a in track.artists)
                parts.append(f"Artists: {artist_names}")

            # Album
            album_obj = album or track.album
            if album_obj:
                parts.append(f"Album: {album_obj.title}")

            # Track/Disc number
            if track.volumeNumber > 1:
                parts.append(f"Track: {track.volumeNumber}-{track.trackNumber}")
            else:
                parts.append(f"Track: {track.trackNumber}")

            # Quality
            quality_tags = []
            if "HIRES_LOSSLESS" in track.mediaMetadata.tags:
                quality_tags.append("Hi-Res")
            elif "LOSSLESS" in track.mediaMetadata.tags:
                quality_tags.append("Lossless")
            if "DOLBY_ATMOS" in track.mediaMetadata.tags:
                quality_tags.append("Atmos")
            if quality_tags:
                parts.append(f"Quality: {', '.join(quality_tags)}")

            # Duration
            duration_min = track.duration // 60
            duration_sec = track.duration % 60
            parts.append(f"Duration: {duration_min}:{duration_sec:02d}")

            # Additional metadata
            if track.bpm:
                parts.append(f"BPM: {track.bpm}")
            if track.key:
                parts.append(f"Key: {track.key}")
            if track.explicit:
                parts.append("Explicit")

            return " | ".join(parts)

        def format_video_line(video: Video) -> str:
            """Format video information as a single line."""
            parts = [
                f"ID: {video.id}",
                f"Title: {video.title}",
            ]

            # Artist(s)
            if video.artist:
                parts.append(f"Artist: {video.artist.name}")
            elif video.artists:
                artist_names = ", ".join(a.name for a in video.artists)
                parts.append(f"Artists: {artist_names}")

            # Quality
            if video.quality:
                parts.append(f"Quality: {video.quality}")

            # Duration
            duration_min = video.duration // 60
            duration_sec = video.duration % 60
            parts.append(f"Duration: {duration_min}:{duration_sec:02d}")

            if video.explicit:
                parts.append("Explicit")

            return " | ".join(parts)

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
            ctx.obj.console.print(f"\n[bold]Album:[/] {album.title}")
            ctx.obj.console.print(
                f"[bold]Artist:[/] {album.artist.name if album.artist else 'Various'}"
            )
            ctx.obj.console.print(f"[bold]Tracks:[/] {album.numberOfTracks}\n")

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
                        ctx.obj.console.print(
                            f"[cyan][VIDEO][/] {format_video_line(item)}"
                        )
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
            ctx.obj.console.print(f"\n[bold]Playlist:[/] {playlist.title}")
            ctx.obj.console.print(f"[bold]Tracks:[/] {playlist.numberOfTracks}\n")

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
                        ctx.obj.console.print(
                            f"[cyan][VIDEO][/] {format_video_line(item)}"
                        )
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
            """List all albums and tracks by an artist."""
            artist = ctx.obj.api.get_artist(resource.id)
            ctx.obj.console.print(f"\n[bold]Artist:[/] {artist.name}\n")

            # List albums
            ctx.obj.console.print("[bold]Albums:[/]")
            offset = 0
            while True:
                artist_albums = ctx.obj.api.get_artist_albums(
                    artist_id=artist.id,
                    filter="ALBUMS",
                    offset=offset,
                )

                for album in artist_albums.items:
                    ctx.obj.console.print(
                        f"  • {album.title} ({album.releaseDate.year}) - {album.numberOfTracks} tracks"
                    )

                if len(artist_albums.items) < artist_albums.limit:
                    break

                offset += artist_albums.limit

            # List singles/EPs if requested
            if SINGLES_FILTER in ["epsandsingles", "all"]:
                ctx.obj.console.print("\n[bold]EPs & Singles:[/]")
                offset = 0
                while True:
                    artist_singles = ctx.obj.api.get_artist_albums(
                        artist_id=artist.id,
                        filter="EPSANDSINGLES",
                        offset=offset,
                    )

                    for album in artist_singles.items:
                        ctx.obj.console.print(
                            f"  • {album.title} ({album.releaseDate.year}) - {album.numberOfTracks} tracks"
                        )

                    if len(artist_singles.items) < artist_singles.limit:
                        break

                    offset += artist_singles.limit

            # List videos if requested
            if VIDEOS_FILTER in ["allow", "only"]:
                ctx.obj.console.print("\n[bold]Videos:[/]")
                offset = 0
                while True:
                    artist_videos = ctx.obj.api.get_artist_videos(
                        artist_id=artist.id,
                        offset=offset,
                    )

                    for video in artist_videos.items:
                        ctx.obj.console.print(f"  • {format_video_line(video)}")

                    if len(artist_videos.items) < artist_videos.limit:
                        break

                    offset += artist_videos.limit

        def list_mix(resource: TidalResource):
            """List all tracks in a mix."""
            ctx.obj.console.print(f"\n[bold]Mix:[/] {resource.id}\n")

            offset = 0
            while True:
                mix_items = ctx.obj.api.get_mix_items(mix_id=resource.id, offset=offset)

                for mix_item in mix_items.items:
                    item = mix_item.item

                    if isinstance(item, Video):
                        if VIDEOS_FILTER == "none":
                            continue
                        ctx.obj.console.print(
                            f"[cyan][VIDEO][/] {format_video_line(item)}"
                        )
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
