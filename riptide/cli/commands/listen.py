import asyncio
import logging
import queue
import threading

import typer
from flask import Flask, abort, jsonify, request
from rich.console import Console
from typing_extensions import Annotated

from riptide.cli.commands.auth import refresh
from riptide.cli.config import CONFIG
from riptide.cli.ctx import Context, ContextObject
from riptide.cli.utils.downloader import ResourceDownloader
from riptide.cli.utils.resource import TidalResource

log = logging.getLogger(__name__)

listen_command = typer.Typer(name="listen")

# Global queue and tracking
download_queue: queue.Queue = queue.Queue()
pending_urls: set[str] = set()
queue_lock = threading.Lock()
verbose_logging = False


class DownloadTask:
    """Represents a single download task."""

    def __init__(self, url: str, resource: TidalResource):
        self.url = url
        self.resource = resource


def process_download_queue(ctx_obj: ContextObject):
    """Background worker that processes downloads sequentially."""
    log.info("Download queue worker started")
    if verbose_logging:
        log.info(f"Worker thread ID: {threading.current_thread().ident}")
        log.info(
            f"Using {CONFIG.listener.concurrent_downloads} threads for parallel track downloads"
        )

    while True:
        try:
            if verbose_logging:
                log.info(
                    f"Waiting for tasks in queue (current size: {download_queue.qsize()})"
                )

            task: DownloadTask = download_queue.get()

            log.info(f"Processing download: {task.url}")
            if verbose_logging:
                log.info(
                    f"Resource type: {task.resource.type}, Resource ID: {task.resource.id}"
                )

            try:
                # Create ResourceDownloader for this download
                if verbose_logging:
                    log.info("Creating ResourceDownloader instance")

                # Create a simple console without Rich progress UI for background downloads
                console = Console()
                downloader = ResourceDownloader(
                    ctx_obj=ctx_obj,
                    console=console,
                    show_progress=False,  # No Rich UI in background
                    threads_count=CONFIG.listener.concurrent_downloads,  # Use listener config for threads
                )

                # Run the download using asyncio
                if verbose_logging:
                    log.info("Starting download_resource")

                loop = asyncio.new_event_loop()
                asyncio.set_event_loop(loop)
                loop.run_until_complete(downloader.download_resource(task.resource))
                loop.close()

                log.info(f"Completed download: {task.url}")

            except Exception as e:
                log.error(f"Error downloading {task.url}: {e}", exc_info=True)

            finally:
                # Remove from pending set
                with queue_lock:
                    pending_urls.discard(task.url)
                    if verbose_logging:
                        log.info(
                            f"Removed from pending set. Remaining pending URLs: {len(pending_urls)}"
                        )

                # Mark task as done
                download_queue.task_done()
                if verbose_logging:
                    log.info(
                        f"Task marked as done. Queue size: {download_queue.qsize()}"
                    )

        except Exception as e:
            log.error(f"Unexpected error in queue worker: {e}", exc_info=True)


def create_flask_app(ctx_obj: ContextObject) -> Flask:
    """Create and configure the Flask application."""
    app = Flask(__name__)

    @app.after_request
    def add_cors(resp):
        resp.headers["Access-Control-Allow-Origin"] = "*"
        resp.headers["Access-Control-Allow-Headers"] = "Content-Type, X-Auth"
        resp.headers["Access-Control-Allow-Methods"] = "POST, OPTIONS"
        return resp

    @app.route("/download", methods=["OPTIONS"])
    def options_download():
        return ("", 204)

    @app.route("/download", methods=["POST"])
    def download():
        if verbose_logging:
            log.info(f"Received POST request from {request.remote_addr}")
            log.info(f"Request headers: {dict(request.headers)}")

        # Check authentication only if secret is configured
        if CONFIG.listener.secret:
            auth_header = request.headers.get("X-Auth", "")
            if verbose_logging:
                log.info(
                    f"Authentication check: secret configured, checking X-Auth header"
                )
            if auth_header != CONFIG.listener.secret:
                log.warning(f"Unauthorized request from {request.remote_addr}")
                abort(403, "Unauthorized")
            if verbose_logging:
                log.info("Authentication successful")
        else:
            if verbose_logging:
                log.info("Authentication skipped: no secret configured")

        # Parse request
        data: dict = {}
        try:
            if verbose_logging:
                log.info(f"Request body (raw): {request.get_data(as_text=True)}")
            request_data = request.get_json(force=True)
            if request_data is not None:
                data = request_data
            if verbose_logging:
                log.info(f"Parsed JSON data: {data}")
        except Exception as e:
            log.error(f"Invalid JSON in request: {e}")
            abort(400, "Invalid JSON")

        url = (data.get("url") or "").strip()
        if verbose_logging:
            log.info(f"Extracted URL: {url}")
        if not url:
            log.error("URL field is missing or empty")
            abort(400, "URL is required")

        # Parse the URL into a TidalResource
        try:
            if verbose_logging:
                log.info(f"Parsing URL into TidalResource: {url}")
            resource = TidalResource.from_string(url)
            if verbose_logging:
                log.info(f"Parsed resource - type: {resource.type}, id: {resource.id}")
        except ValueError as e:
            log.error(f"Invalid URL: {url} - {e}")
            abort(400, f"Invalid URL: {e}")
            return  # This line is never reached due to abort, but helps type checker

        # Check if already in queue
        with queue_lock:
            if url in pending_urls:
                log.info(f"URL already in queue: {url}")
                return jsonify(status="accepted", message="Already in queue"), 202

            # Add to pending set
            pending_urls.add(url)
            if verbose_logging:
                log.info(f"Added to pending set. Total pending: {len(pending_urls)}")

        # Add to queue
        task = DownloadTask(url=url, resource=resource)
        download_queue.put(task)
        if verbose_logging:
            log.info(f"Added task to queue. Queue size: {download_queue.qsize()}")

        log.info(f"Added to queue: {url}")
        return jsonify(status="accepted"), 202

    return app


@listen_command.callback(invoke_without_command=True)
def listen_callback(
    ctx: Context,
    verbose: Annotated[
        bool,
        typer.Option(
            "--verbose",
            "-v",
            help="Enable verbose logging for debugging",
        ),
    ] = False,
):
    """
    Start the listener server to accept download requests via API.

    The listener accepts POST requests to /download with a JSON body containing a "url" field.
    Authentication is required via X-Auth header matching the configured secret.

    Downloads requests are processed sequentially in a queue to prevent concurrent downloads.
    """

    # Set verbose logging flag
    global verbose_logging
    verbose_logging = verbose

    # Initialize console handler for later use
    console_handler = None

    if verbose:
        # Set logging level to DEBUG for verbose output
        riptide_logger = logging.getLogger("riptide")
        riptide_logger.setLevel(logging.DEBUG)
        log.setLevel(logging.DEBUG)

        # Add console handler for verbose output
        console_handler = logging.StreamHandler()
        console_handler.setLevel(logging.DEBUG)
        console_handler.setFormatter(
            logging.Formatter("%(levelname)s:%(name)s:%(message)s")
        )
        riptide_logger.addHandler(console_handler)

        ctx.obj.console.print("[cyan]Verbose logging enabled[/]")
        log.debug("Debug logging is active")

    # Warn if no secret is configured
    if not CONFIG.listener.secret:
        ctx.obj.console.print(
            "[yellow]Warning:[/] Listener secret is not configured. "
            "The API will accept requests without authentication. "
            "Set [yellow]listener.secret[/] in your config.toml file for security."
        )
        if verbose:
            log.debug("No authentication secret configured")

    # Refresh authentication
    if verbose:
        log.debug("Refreshing Tidal authentication")
    refresh(ctx)
    if verbose:
        log.debug("Authentication refresh completed")

    port = CONFIG.listener.port

    ctx.obj.console.print(f"[green]Starting listener on 127.0.0.1:{port}[/]")
    ctx.obj.console.print(
        f"[blue]Send POST requests to http://127.0.0.1:{port}/download[/]"
    )
    if CONFIG.listener.secret:
        ctx.obj.console.print(
            "[yellow]Authentication enabled: Include X-Auth header with your configured secret[/]"
        )
    else:
        ctx.obj.console.print(
            "[yellow]Authentication disabled: No X-Auth header required[/]"
        )

    if verbose:
        log.debug(
            f"Configuration: port={port}, threads={CONFIG.download.threads_count}"
        )
        log.debug(f"Download path: {CONFIG.download.download_path}")
        log.debug(f"Track quality: {CONFIG.download.track_quality}")
        log.debug(f"Concurrent track downloads: {CONFIG.listener.concurrent_downloads}")

    # Start the queue worker thread
    if verbose:
        log.debug("Starting queue worker thread")
    worker_thread = threading.Thread(
        target=process_download_queue, args=(ctx.obj,), daemon=True
    )
    worker_thread.start()
    if verbose:
        log.debug(f"Worker thread started with ID: {worker_thread.ident}")

    # Create and run Flask app
    if verbose:
        log.debug("Creating Flask application")
    app = create_flask_app(ctx.obj)

    # Configure Flask/Werkzeug logging
    werkzeug_logger = logging.getLogger("werkzeug")
    if verbose and console_handler:
        # Enable Flask request logging
        werkzeug_logger.setLevel(logging.INFO)
        werkzeug_logger.addHandler(console_handler)
    else:
        # Suppress Flask default logging
        werkzeug_logger.setLevel(logging.ERROR)

    # Run the server
    try:
        if verbose:
            log.debug("Starting Flask server")
        app.run(host="127.0.0.1", port=port, debug=False, use_reloader=False)
    except KeyboardInterrupt:
        ctx.obj.console.print("\n[yellow]Shutting down listener...[/]")
        if verbose:
            log.debug("Received keyboard interrupt")
    except Exception as e:
        ctx.obj.console.print(f"[red]Error:[/] {e}")
        if verbose:
            log.exception("Exception during server runtime")
        raise typer.Exit(1)
