"""
Utility for downloading Tidal resources.

This module provides a reusable ResourceDownloader class that encapsulates
the logic for downloading tracks, albums, playlists, mixes, artists, and videos.
"""

import asyncio
import logging
import os
from pathlib import Path
from types import SimpleNamespace
from typing import Optional

from rich.console import Console
from rich.live import Live

from riptide.cli.config import (
    ARTIST_SINGLES_FILTER_LITERAL,
    CONFIG,
    TRACK_QUALITY_LITERAL,
    VIDEO_QUALITY_LITERAL,
    VIDEOS_FILTER_LITERAL,
)
from riptide.cli.ctx import ContextObject
from riptide.cli.utils.resource import TidalResource
from riptide.core.api import ApiError
from riptide.core.api.models import Album, AlbumItemsCredits, Track, Video
from riptide.core.metadata import (
    Cover,
    add_track_metadata,
    add_video_metadata,
    save_lyrics_to_lrc,
)
from riptide.core.utils.format import format_template
from riptide.core.utils.m3u import save_tracks_to_m3u

from ..commands.download.downloader import Downloader
from ..commands.download.output import RichOutput

log = logging.getLogger(__name__)


class ResourceDownloader:
    """
    Handles downloading of Tidal resources with consistent configuration.

    This class can be used by both the CLI download command and the listener
    to ensure consistent behavior.
    """

    def __init__(
        self,
        ctx_obj: ContextObject,
        track_quality: TRACK_QUALITY_LITERAL = CONFIG.download.track_quality,
        video_quality: VIDEO_QUALITY_LITERAL = CONFIG.download.video_quality,
        skip_existing: bool = CONFIG.download.skip_existing,
        rewrite_metadata: bool = CONFIG.download.rewrite_metadata,
        threads_count: int = CONFIG.download.threads_count,
        download_path: Path = CONFIG.download.download_path,
        scan_path: Path = CONFIG.download.scan_path,
        template: str = "",
        singles_filter: ARTIST_SINGLES_FILTER_LITERAL = CONFIG.download.singles_filter,
        videos_filter: VIDEOS_FILTER_LITERAL = CONFIG.download.videos_filter,
        skip_errors: bool = False,
        console: Optional[Console] = None,
        show_progress: bool = True,
    ):
        """
        Initialize the ResourceDownloader.

        Args:
            ctx_obj: Context object with API access
            track_quality: Quality setting for tracks
            video_quality: Quality setting for videos
            skip_existing: Whether to skip already downloaded files
            rewrite_metadata: Whether to rewrite metadata for existing files
            threads_count: Number of concurrent download threads
            download_path: Base directory for downloads
            scan_path: Directory to scan for existing downloads
            template: Custom output template (empty string uses config defaults)
            singles_filter: Filter for artist singles
            videos_filter: Filter for videos
            skip_errors: Whether to continue on errors
            console: Rich console for output (None to use ctx_obj.console)
            show_progress: Whether to show Rich progress UI
        """
        self.ctx_obj = ctx_obj
        self.track_quality = track_quality
        self.video_quality = video_quality
        self.skip_existing = skip_existing
        self.rewrite_metadata = rewrite_metadata
        self.threads_count = threads_count
        self.download_path = download_path
        self.scan_path = scan_path
        self.template = template
        self.singles_filter = singles_filter
        self.videos_filter = videos_filter
        self.skip_errors = skip_errors
        self.console = console or ctx_obj.console
        self.show_progress = show_progress

        self.rich_output = RichOutput(self.console)
        self.downloader = Downloader(
            tidal_api=ctx_obj.api,
            threads_count=threads_count,
            rich_output=self.rich_output,
            track_quality=track_quality,
            video_quality=video_quality,
            videos_filter=videos_filter,
            skip_existing=skip_existing,
            download_path=download_path,
            scan_path=scan_path,
        )

    def get_item_quality(self, item: Track | Video) -> str:
        """Get the quality string for an item."""
        if isinstance(item, Track):
            if self.track_quality in ["low", "normal"]:
                return self.track_quality.upper()

            if (
                self.track_quality == "max"
                and "HIRES_LOSSLESS" not in item.mediaMetadata.tags
            ):
                return "HIGH"

            return self.track_quality.upper()

        elif isinstance(item, Video):
            return self.video_quality.upper()

        raise TypeError("Unsupported item type")

    def save_m3u(
        self,
        resource_type: str,
        filename: str,
        tracks_with_path: list[tuple[Path, Track]],
    ):
        """Save an M3U playlist file if configured."""
        if not CONFIG.m3u.save:
            return

        if resource_type not in CONFIG.m3u.allowed:
            return

        tracks_with_existing_paths = [
            (path, track)
            for (path, track) in tracks_with_path
            if path and isinstance(track, Track)
        ]

        log.debug(f"{resource_type=}, {filename=}, {len(tracks_with_existing_paths)=}")

        save_tracks_to_m3u(
            tracks_with_path=tracks_with_existing_paths,
            path=self.download_path / filename,
        )

    async def download_resource(self, resource: TidalResource) -> None:
        """
        Download a single Tidal resource.

        Args:
            resource: The TidalResource to download

        Raises:
            ApiError: If there's an API error
            Exception: For other errors
        """
        if resource.type == "track":
            await self._download_track(resource)
        elif resource.type == "video":
            await self._download_video(resource)
        elif resource.type == "album":
            await self._download_album(resource)
        elif resource.type == "playlist":
            await self._download_playlist(resource)
        elif resource.type == "mix":
            await self._download_mix(resource)
        elif resource.type == "artist":
            await self._download_artist(resource)
        else:
            raise ValueError(f"Unsupported resource type: {resource.type}")

    async def download_resources(self, resources: list[TidalResource]) -> None:
        """
        Download multiple Tidal resources.

        Args:
            resources: List of TidalResource objects to download
        """

        async def wrapper(r: TidalResource):
            try:
                await self.download_resource(r)
            except ApiError as e:
                self.console.print(f"[red]API Error:[/] {e} ({r})")
                if not self.skip_errors:
                    raise
            except Exception as e:
                self.console.print(f"[red]Error:[/] {e} ({r})")
                if not self.skip_errors:
                    raise

        if self.show_progress:
            with Live(
                self.rich_output.group,
                refresh_per_second=10,
                console=self.console,
                transient=True,
            ):
                await asyncio.gather(*(wrapper(r) for r in resources))
            self.rich_output.show_stats()
        else:
            await asyncio.gather(*(wrapper(r) for r in resources))

    async def _download_track(self, resource: TidalResource) -> None:
        """Download a single track."""
        track = self.ctx_obj.api.get_track(int(resource.id))
        template = self.template or CONFIG.download.templates.track
        file_path = format_template(
            template=template,
            item=track,
            quality=self.get_item_quality(track),
        )

        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=track, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            await self._add_track_metadata(track, download_path)

        if download_path and CONFIG.download.update_mtime:
            try:
                os.utime(download_path, None)
            except Exception:
                log.warning(f"could not update mtime for {download_path}")

    async def _download_video(self, resource: TidalResource) -> None:
        """Download a single video."""
        video = self.ctx_obj.api.get_video(int(resource.id))
        template = self.template or CONFIG.download.templates.video
        file_path = format_template(
            template=template,
            item=video,
            quality=self.get_item_quality(video),
        )

        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=video, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            add_video_metadata(path=download_path, video=video)

        if download_path and CONFIG.download.update_mtime:
            try:
                os.utime(download_path, None)
            except Exception:
                log.warning(f"could not update mtime for {download_path}")

    async def _download_album_track(
        self,
        track: Track,
        file_path: str,
        album_metadata,
    ) -> tuple[Path | None, Track]:
        """Download a single track from an album."""
        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=track, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            await self._add_track_metadata(
                track,
                download_path,
                cover=album_metadata.cover,
                album_review=album_metadata.album_review,
                album=None,
                credits=album_metadata.credits,
            )

        return download_path, track

    async def _download_album_video(
        self,
        video: Video,
        file_path: str,
        album_metadata,
    ) -> None:
        """Download a single video from an album."""
        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=video, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            add_video_metadata(path=download_path, video=video)

    async def _download_album(self, resource: TidalResource) -> None:
        """Download an entire album."""
        album = self.ctx_obj.api.get_album(int(resource.id))
        offset = 0

        cover: Cover | None = None
        save_cover = ("album" in CONFIG.cover.allowed) and CONFIG.cover.save

        if album.cover and (CONFIG.metadata.cover or save_cover):
            cover = Cover(album.cover, size=CONFIG.cover.size)

        album_review = ""
        if CONFIG.metadata.album_review:
            try:
                album_review = self.ctx_obj.api.get_album_review(
                    album_id=resource.id
                ).normalized_text()
            except Exception as e:
                log.error(e)

        tracks_for_m3u: list[tuple[Path, Track]] = []

        while True:
            album_items = self.ctx_obj.api.get_album_items_credits(
                album_id=album.id, offset=offset
            )

            # Separate tracks and videos, collect futures
            track_futures = []
            video_futures = []

            for album_item in album_items.items:
                try:
                    template = self.template or CONFIG.download.templates.album
                    file_path = format_template(
                        template=template,
                        item=album_item.item,
                        album=album,
                        quality=self.get_item_quality(album_item.item),
                    )

                    # Update metadata with credits for this item
                    item_metadata = SimpleNamespace(
                        cover=cover,
                        album_review=album_review,
                        credits=album_item.credits,
                    )

                    if isinstance(album_item.item, Track):
                        track_futures.append(
                            self._download_album_track(
                                album_item.item, file_path, item_metadata
                            )
                        )
                    elif isinstance(album_item.item, Video):
                        video_futures.append(
                            self._download_album_video(
                                album_item.item, file_path, item_metadata
                            )
                        )

                except Exception as e:
                    log.error(f"Failed to prepare download {album_item.item}: {e}")
                    if not self.skip_errors:
                        raise

            offset += album_items.limit
            if offset >= album_items.totalNumberOfItems:
                break

        # Download all tracks concurrently
        if track_futures:
            track_results = await asyncio.gather(*track_futures)
            tracks_for_m3u = [
                (path, track) for path, track in track_results if path is not None
            ]

        # Download all videos concurrently
        if video_futures:
            await asyncio.gather(*video_futures)

        # Save album cover if configured
        if save_cover and cover:
            template = self.template or CONFIG.cover.templates.album
            if template:
                cover_file_path = format_template(
                    template=template,
                    album=album,
                )
                cover_path = self.download_path / f"{cover_file_path}.jpg"
                cover_path.parent.mkdir(parents=True, exist_ok=True)
                cover.save_to_directory(cover_path.parent)

        # Save M3U playlist
        if tracks_for_m3u:
            template = self.template or CONFIG.m3u.templates.album
            if template:
                m3u_filename = format_template(
                    template=template, album=album, type="album"
                )
                self.save_m3u("album", m3u_filename, tracks_for_m3u)

    async def _download_playlist_track(
        self, track: Track, file_path: str
    ) -> tuple[Path | None, Track]:
        """Download a single track from a playlist."""
        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=track, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            await self._add_track_metadata(track, download_path)

        return download_path, track

    async def _download_playlist_video(self, video: Video, file_path: str) -> None:
        """Download a single video from a playlist."""
        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=video, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            add_video_metadata(path=download_path, video=video)

    async def _download_playlist(self, resource: TidalResource) -> None:
        """Download a playlist."""
        playlist = self.ctx_obj.api.get_playlist(resource.id)
        offset = 0
        tracks_for_m3u: list[tuple[Path, Track]] = []

        cover: Cover | None = None
        save_cover = ("playlist" in CONFIG.cover.allowed) and CONFIG.cover.save

        if save_cover:
            if playlist.squareImage:
                cover = Cover(playlist.squareImage, size=min(CONFIG.cover.size, 1080))
            elif playlist.image:
                cover = Cover(playlist.image, size=min(CONFIG.cover.size, 1080))

        # Collect futures for concurrent download
        track_futures = []
        video_futures = []

        while True:
            playlist_items = self.ctx_obj.api.get_playlist_items(
                playlist_uuid=resource.id, offset=offset
            )

            for playlist_item in playlist_items.items:
                try:
                    template = self.template or CONFIG.download.templates.playlist
                    file_path = format_template(
                        template=template,
                        item=playlist_item,
                        playlist=playlist,
                        quality=self.get_item_quality(playlist_item),
                    )

                    if isinstance(playlist_item, Track):
                        track_futures.append(
                            self._download_playlist_track(playlist_item, file_path)
                        )
                    elif isinstance(playlist_item, Video):
                        video_futures.append(
                            self._download_playlist_video(playlist_item, file_path)
                        )

                except Exception as e:
                    log.error(f"Failed to prepare download {playlist_item}: {e}")
                    if not self.skip_errors:
                        raise

            offset += playlist_items.limit
            if offset >= playlist_items.totalNumberOfItems:
                break

        # Download all tracks concurrently
        if track_futures:
            track_results = await asyncio.gather(*track_futures)
            tracks_for_m3u = [
                (path, track) for path, track in track_results if path is not None
            ]

        # Download all videos concurrently
        if video_futures:
            await asyncio.gather(*video_futures)

        # Save playlist cover
        if save_cover and cover:
            template = self.template or CONFIG.cover.templates.playlist
            if template:
                cover_file_path = format_template(
                    template=template,
                    playlist=playlist,
                )
                cover_path = self.download_path / f"{cover_file_path}.jpg"
                cover_path.parent.mkdir(parents=True, exist_ok=True)
                cover.save_to_directory(cover_path.parent)

        # Save M3U playlist
        if tracks_for_m3u:
            template = self.template or CONFIG.m3u.templates.playlist
            if template:
                m3u_filename = format_template(
                    template=template, playlist=playlist, type="playlist"
                )
                self.save_m3u("playlist", m3u_filename, tracks_for_m3u)

    async def _download_mix_track(
        self, mix_item: Track, file_path: str
    ) -> tuple[Path | None, Track]:
        """Download a single track from a mix."""
        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=mix_item, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            await self._add_track_metadata(mix_item, download_path)

        return download_path, mix_item

    async def _download_mix_video(self, mix_item: Video, file_path: str) -> None:
        """Download a single video from a mix."""
        self.rich_output.total_increment()

        download_path, was_downloaded = await self.downloader.download(
            item=mix_item, file_path=Path(file_path)
        )

        if (
            CONFIG.metadata.enable
            and download_path
            and (self.rewrite_metadata or was_downloaded)
        ):
            add_video_metadata(path=download_path, video=mix_item)

    async def _download_mix(self, resource: TidalResource) -> None:
        """Download a mix."""
        # Note: There's no get_mix method, we just use the mix_id directly
        offset = 0
        tracks_for_m3u: list[tuple[Path, Track]] = []

        while True:
            mix_items = self.ctx_obj.api.get_mix_items(
                mix_id=resource.id, offset=offset
            )

            # Collect futures for concurrent download
            track_futures = []
            video_futures = []

            for mix_item in mix_items.items:
                try:
                    template = self.template or CONFIG.download.templates.mix

                    # Check if template needs album info
                    album = None
                    if "{album" in template:
                        try:
                            album = self.ctx_obj.api.get_album(mix_item.album.id)
                        except Exception as e:
                            log.error(f"Failed to fetch album: {e}")

                    file_path = format_template(
                        template=template,
                        item=mix_item,
                        album=album,
                        mix_id=resource.id,
                        quality=self.get_item_quality(mix_item),
                    )

                    if isinstance(mix_item, Track):
                        track_futures.append(
                            self._download_mix_track(mix_item, file_path)
                        )
                    elif isinstance(mix_item, Video):
                        video_futures.append(
                            self._download_mix_video(mix_item, file_path)
                        )

                except Exception as e:
                    log.error(f"Failed to prepare download {mix_item}: {e}")
                    if not self.skip_errors:
                        raise

            offset += mix_items.limit
            if offset >= mix_items.totalNumberOfItems:
                break

        # Download all tracks concurrently
        if track_futures:
            track_results = await asyncio.gather(*track_futures)
            tracks_for_m3u = [
                (path, track) for path, track in track_results if path is not None
            ]

        # Download all videos concurrently
        if video_futures:
            await asyncio.gather(*video_futures)

        # Save M3U playlist
        if tracks_for_m3u:
            template = self.template or CONFIG.m3u.templates.mix
            if template:
                m3u_filename = format_template(
                    template=template, mix_id=resource.id, type="mix"
                )
                self.save_m3u("mix", m3u_filename, tracks_for_m3u)

    async def _download_artist(self, resource: TidalResource) -> None:
        """Download all albums from an artist."""
        artist = self.ctx_obj.api.get_artist(int(resource.id))
        offset = 0

        all_albums = []
        while True:
            artist_albums = self.ctx_obj.api.get_artist_albums(
                artist_id=artist.id, offset=offset
            )

            for album in artist_albums.items:
                # Apply singles filter
                is_single = album.numberOfTracks == 1

                if self.singles_filter == "none" and is_single:
                    continue
                elif self.singles_filter == "only" and not is_single:
                    continue

                all_albums.append(album)

            offset += artist_albums.limit
            if offset >= artist_albums.totalNumberOfItems:
                break

        # Download each album
        for album in all_albums:
            album_resource = TidalResource(type="album", id=str(album.id))
            await self._download_album(album_resource)

    async def _add_track_metadata(
        self,
        track: Track,
        download_path: Path,
        cover: Cover | None = None,
        album_review: str = "",
        album: Album | None = None,
        credits: list[AlbumItemsCredits.ItemWithCredits.CreditsEntry] | None = None,
    ) -> None:
        """Add metadata to a downloaded track."""
        lyrics_subtitles = ""
        track_lyrics = None

        if CONFIG.metadata.lyrics or CONFIG.metadata.save_lyrics_to_lrc:
            try:
                track_lyrics = self.ctx_obj.api.get_track_lyrics(track.id)
                lyrics_subtitles = track_lyrics.subtitles
            except Exception as e:
                log.error(e)

        # Get cover if not provided
        if not cover and track.album.cover and CONFIG.metadata.cover:
            cover = Cover(track.album.cover, size=CONFIG.cover.size)

        if cover and cover.data is None:
            cover.fetch_data()

        # Get album artist
        album_artist = ""
        if album:
            album_artist = album.artist.name if album.artist else ""
        elif track.album:
            # Fetch album to get artist info
            try:
                full_album = self.ctx_obj.api.get_album(track.album.id)
                album_artist = full_album.artist.name if full_album.artist else ""
            except Exception as e:
                log.error(f"Failed to fetch album for track metadata: {e}")

        add_track_metadata(
            path=download_path,
            track=track,
            lyrics=lyrics_subtitles,
            album_artist=album_artist,
            cover_data=(cover.data if cover else None),
            date=str(album.releaseDate) if album else "",
            credits_contributors=credits if credits else [],
            comment=album_review,
        )

        # Save lyrics to .lrc file if enabled
        if CONFIG.metadata.save_lyrics_to_lrc and track_lyrics:
            try:
                save_lyrics_to_lrc(download_path, track_lyrics.subtitles)
            except Exception as e:
                log.error(f"Failed to save lyrics: {e}")
