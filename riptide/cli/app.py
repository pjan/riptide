import logging

import typer
from rich.console import Console
from typing_extensions import Annotated

from riptide.cli.commands import register_commands
from riptide.cli.config import APP_PATH, CONFIG
from riptide.cli.ctx import Context, ContextObject
from riptide.core.utils.ffmpeg import is_ffmpeg_installed as ifs

log = logging.getLogger("riptide")

app = typer.Typer(name="riptide", no_args_is_help=True, rich_markup_mode="rich")
register_commands(app)


@app.callback()
def callback(
    ctx: Context,
    OMIT_CACHE: Annotated[
        bool,
        typer.Option(
            "--omit-cache",
        ),
    ] = not CONFIG.enable_cache,
    DEBUG: Annotated[
        bool,
        typer.Option(
            "--debug",
        ),
    ] = CONFIG.debug,
):
    """
    riptide - a CLI to download tidal tracks \u266b

    [link=https://github.com/pjan/riptide]github[/link] \u2764
    """

    log.debug(f"{ctx.params=}")

    is_ffmpeg_installed = ifs()
    log.debug(f"{is_ffmpeg_installed=}")

    if DEBUG:
        debug_path = APP_PATH / "api_debug"
    else:
        debug_path = None

    ctx.obj = ContextObject(
        api_omit_cache=OMIT_CACHE, console=Console(), debug_path=debug_path
    )

    if not is_ffmpeg_installed:
        ctx.obj.console.print(
            "[yellow]WARNING ffmpeg is not installed, riptide might not work properly.[/]"
        )
