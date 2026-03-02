"""
Module to create and return a TidalAPI session.

This module provides a reusable function to initialize the Tidal API
with authentication and token refresh capabilities.

Usage:
    from examples.create_api_session import get_api

    api = get_api()
    session = api.get_session()
"""

import sys
from pathlib import Path

# Add parent directory to path to import riptide modules
sys.path.insert(0, str(Path(__file__).parent.parent))

from riptide.cli.const import APP_PATH
from riptide.cli.utils.auth import load_auth_data
from riptide.core.api import TidalAPI, TidalClient
from riptide.core.auth.api import AuthAPI


def get_api() -> TidalAPI:
    """
    Initialize and return TidalAPI instance.

    This function:
    - Loads authentication data from ~/.riptide/auth.json
    - Creates a TidalClient with caching and token refresh
    - Returns a configured TidalAPI instance

    Raises:
        SystemExit: If not logged in or auth data is incomplete

    Returns:
        TidalAPI: Configured API instance ready to use

    Note:
        Remember to login first: `riptide auth login`
        If token expired, refresh it: `riptide auth refresh`
    """
    # Load our token, country code and user id from file
    auth_data = load_auth_data()

    # Ensure we are logged in
    if not auth_data.token:
        print("❌ Not logged in. Please run: riptide auth login")
        raise SystemExit(1)

    if not auth_data.user_id or not auth_data.country_code:
        print("❌ Auth data incomplete. Please run: riptide auth login")
        raise SystemExit(1)

    def on_token_expiry() -> str | None:
        """Refresh token when it expires."""
        if not auth_data.refresh_token:
            return None

        auth_api = AuthAPI()
        auth_response = auth_api.refresh_token(auth_data.refresh_token)
        auth_data.token = auth_response.access_token

        return auth_response.access_token

    # Create Client for our API
    # This is a custom client that can cache requests
    # to make the API more efficient
    client = TidalClient(
        token=auth_data.token,
        cache_name=APP_PATH / "api_cache",  # path to cache api requests
        debug_path=APP_PATH / "api_debug",  # optional, used for debugging api
        on_token_expiry=on_token_expiry,
    )

    # Create and return our Tidal API that will call the endpoints
    api = TidalAPI(
        client=client,
        user_id=auth_data.user_id,
        country_code=auth_data.country_code,
    )

    return api


if __name__ == "__main__":
    # Example usage: make an API call
    api = get_api()
    session = api.get_session()

    # Every data from the api is a `pydantic` model
    print(f"session id: {session.sessionId}")

    # See every available endpoint at `riptide.core.api`
