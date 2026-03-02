#!/usr/bin/env python3
"""
Script to fetch and display all template fields for a Tidal track.

Usage:
    python examples/show_templating_with_track.py <track_id>

Example:
    python examples/show_templating_with_track.py 1845520
"""

import sys
from pathlib import Path

# Add parent directory to path to import riptide modules
sys.path.insert(0, str(Path(__file__).parent.parent))

from examples.create_api_session import get_api
from riptide.core.utils.format import generate_template_data


def format_value(value) -> str:
    """Format a value for display."""
    if value is None:
        return ""
    if isinstance(value, bool):
        return str(value)
    if isinstance(value, (int, float)):
        return str(value)
    if hasattr(value, "__format__"):
        # For Explicit and UserFormat objects
        try:
            # Try different format specs
            formatted = format(value, "")
            if not formatted:
                formatted = format(value, "long")
            if not formatted:
                formatted = format(value, "full")
            return formatted if formatted else "(empty)"
        except:
            return str(value)
    return str(value)


def show_track_info(track_id: int):
    """Fetch and display all template fields for a track."""
    print(f"\n🎵 Fetching track info for ID: {track_id}\n")

    api = get_api()

    # Fetch track
    track = api.get_track(track_id)

    # Fetch album
    album = api.get_album(track.album.id)

    # Generate template data
    template_data = generate_template_data(
        item=track,
        album=album,
        quality=track.audioQuality,
    )

    item = template_data["item"]
    album_data = template_data["album"]

    # Display item (track) fields
    print("=" * 70)
    print("ITEM (TRACK) FIELDS")
    print("=" * 70)

    if item:
        fields = [
            ("item.id", item.id),
            ("item.title", item.title),
            ("item.title_version", item.title_version),
            ("item.number", item.number),
            ("item.volume", item.volume),
            ("item.version", item.version),
            ("item.copyright", item.copyright),
            ("item.bpm", item.bpm),
            ("item.isrc", item.isrc),
            ("item.quality", item.quality),
            ("item.artist", item.artist),
            ("item.artists", item.artists),
            ("item.features", item.features),
            ("item.artists_with_features", item.artists_with_features),
            ("item.explicit", format_value(item.explicit)),
            ("item.explicit:long", format(item.explicit, "long")),
            ("item.explicit:full", format(item.explicit, "full")),
            ("item.dolby:(Dolby Atmos)", format(item.dolby, "(Dolby Atmos)")),
        ]

        max_field_len = max(len(field) for field, _ in fields)

        for field, value in fields:
            formatted_value = (
                format_value(value) if not isinstance(value, str) else value
            )
            print(f"{field:<{max_field_len}} = {formatted_value}")

    # Display album fields
    print("\n" + "=" * 70)
    print("ALBUM FIELDS")
    print("=" * 70)

    if album_data:
        fields = [
            ("album.id", album_data.id),
            ("album.title", album_data.title),
            ("album.artist", album_data.artist),
            ("album.artists", album_data.artists),
            ("album.date", album_data.date),
            ("album.date:%Y-%m-%d", album_data.date.strftime("%Y-%m-%d")),
            ("album.date:%Y", album_data.date.strftime("%Y")),
            ("album.explicit", format_value(album_data.explicit)),
            ("album.explicit:long", format(album_data.explicit, "long")),
            ("album.explicit:full", format(album_data.explicit, "full")),
            ("album.master:[MAX]", format(album_data.master, "[MAX]")),
            ("album.release", album_data.release),
        ]

        max_field_len = max(len(field) for field, _ in fields)

        for field, value in fields:
            formatted_value = (
                format_value(value) if not isinstance(value, str) else value
            )
            print(f"{field:<{max_field_len}} = {formatted_value}")

    print("\n" + "=" * 70)
    print("RAW TRACK DATA (from API)")
    print("=" * 70)
    print(f"Full track object: {track.model_dump_json(indent=2)[:500]}...")

    print("\n✅ Done!\n")


def main():
    if len(sys.argv) != 2:
        print("Usage: python examples/show_track_info.py <track_id>")
        print("Example: python examples/show_track_info.py 1845520")
        sys.exit(1)

    try:
        track_id = int(sys.argv[1])
    except ValueError:
        print(f"❌ Invalid track ID: {sys.argv[1]}")
        print("Track ID must be a number")
        sys.exit(1)

    show_track_info(track_id)


if __name__ == "__main__":
    main()
