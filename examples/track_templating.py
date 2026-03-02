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
from riptide.core.utils.format import format_template

ALBUM_ID = 1845519

if __name__ == "__main__":
    api = get_api()

    album = api.get_album(ALBUM_ID)
    album_items = api.get_album_items(ALBUM_ID)

    TEMPLATE = (
        "{album.artists}/{album.date:%Y} - {album.title}/{item.number:02d} {item.title}"
    )

    for album_item in album_items.items:
        track = album_item.item

        print(
            format_template(
                template=TEMPLATE,
                item=track,
                album=album,
                with_asterisk_ext=False,
                custom_field="custom_field",
            )
        )
