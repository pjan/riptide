# Listener Mode

The listener mode allows riptide to accept download requests via HTTP API, enabling integration with browser extensions, scripts, and other applications.

## Overview

When started, the listener:
- Runs an HTTP server on `localhost` (127.0.0.1)
- Accepts POST requests to `/download` endpoint
- Processes downloads sequentially in a queue
- Prevents duplicate submissions of the same URL
- Uses the same configuration and settings as the CLI download command

## Configuration

Add the following section to your `config.toml`:

```toml
[listener]
port = 8123
secret = ""
concurrent_downloads = 1
```

### Configuration Options

- **`port`** (default: `8123`): The port number for the HTTP server
- **`secret`** (default: `""`): Authentication secret for API requests
  - If empty: Authentication is disabled, all requests are accepted
  - If set: Clients must include `X-Auth` header matching this value
- **`concurrent_downloads`** (default: `1`): Number of tracks to download in parallel
  - Set to `1` for sequential track downloads (one track at a time)
  - Set higher (e.g., `3`, `5`) to download multiple tracks simultaneously within an album/playlist
  - This is the same as `threads_count` in the download command, but specific to the listener

### Security Considerations

- When `secret` is empty, **anyone** who can reach `localhost` can submit downloads
- This is fine for local development/testing, but set a secret for production use
- The server only binds to `127.0.0.1` (localhost), so it's not accessible from other machines by default

## Usage

### Starting the Listener

```bash
# Basic usage
riptide listen

# With verbose logging (recommended for debugging)
riptide listen --verbose
```

### Command Line Options

- `--verbose`, `-v`: Enable detailed logging for debugging

### Stopping the Listener

Press `Ctrl+C` to gracefully shut down the listener.

## API Reference

### POST /download

Submit a URL to download.

#### Request

**Endpoint:** `POST http://127.0.0.1:8123/download`

**Headers:**
- `Content-Type: application/json` (required)
- `X-Auth: <your-secret>` (required only if secret is configured)

**Body:**
```json
{
  "url": "https://tidal.com/browse/track/123456"
}
```

#### Response

**Success (202 Accepted):**
```json
{
  "status": "accepted"
}
```

**Already in Queue (202 Accepted):**
```json
{
  "status": "accepted",
  "message": "Already in queue"
}
```

**Note:** The same URL cannot be queued multiple times while it's in the queue. Once a download completes, the URL can be submitted again.

**Error Responses:**
- `400 Bad Request`: Invalid JSON, missing URL, or invalid URL format
- `403 Forbidden`: Missing or incorrect X-Auth header (when secret is configured)

### OPTIONS /download

CORS preflight request support.

**Response:** `204 No Content`

## Supported URL Formats

The listener supports all resource types that the CLI supports:

- **Tracks:** `https://tidal.com/browse/track/123456` or `track/123456`
- **Albums:** `https://tidal.com/browse/album/123456` or `album/123456`
- **Playlists:** `https://tidal.com/browse/playlist/uuid` or `playlist/uuid`
- **Mixes:** `https://tidal.com/browse/mix/mixid` or `mix/mixid`
- **Artists:** `https://tidal.com/browse/artist/123456` or `artist/123456`
- **Videos:** `https://tidal.com/browse/video/123456` or `video/123456`

## Examples

### Using curl (No Authentication)

```bash
# Simple track download
curl -X POST http://127.0.0.1:8123/download \
  -H "Content-Type: application/json" \
  -d '{"url": "https://tidal.com/browse/track/123456"}'

# Using shorthand format
curl -X POST http://127.0.0.1:8123/download \
  -H "Content-Type: application/json" \
  -d '{"url": "track/123456"}'
```

### Using curl (With Authentication)

```bash
curl -X POST http://127.0.0.1:8123/download \
  -H "Content-Type: application/json" \
  -H "X-Auth: your-secret-key-here" \
  -d '{"url": "https://tidal.com/browse/album/123456"}'
```

### Using JavaScript (Browser Extension)

```javascript
async function submitDownload(url) {
  const response = await fetch('http://127.0.0.1:8123/download', {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'X-Auth': 'your-secret-key-here'  // omit if no secret configured
    },
    body: JSON.stringify({ url: url })
  });
  
  if (response.ok) {
    const data = await response.json();
    console.log('Download queued:', data.status);
  } else {
    console.error('Failed to queue download:', response.status);
  }
}

// Usage
submitDownload('https://tidal.com/browse/track/123456');
```

### Using Python

```python
import requests

def submit_download(url, secret=None):
    headers = {'Content-Type': 'application/json'}
    if secret:
        headers['X-Auth'] = secret
    
    response = requests.post(
        'http://127.0.0.1:8123/download',
        headers=headers,
        json={'url': url}
    )
    
    if response.status_code == 202:
        print(f"Download queued: {response.json()}")
    else:
        print(f"Error: {response.status_code} - {response.text}")

# Usage
submit_download('https://tidal.com/browse/album/123456', secret='your-secret')
```

## Troubleshooting

### First-Run Checklist

If you're running the listener for the first time, ensure:

1. **Tidal authentication is set up:**
   ```bash
   riptide auth login
   ```

2. **Configuration file exists:**
   - Location: `~/.riptide/config.toml` (Linux/macOS) or `C:\Users\<username>\.riptide\config.toml` (Windows)
   - Add listener section if needed:
   ```toml
   [listener]
   port = 8123
   secret = ""  # Optional
   ```

3. **Download path is configured:**
   - Check `[download]` section in config.toml
   - Default: `~/Downloads/riptide`

### Downloads Not Starting

1. **Check if the listener is running:**
   ```bash
   riptide listen --verbose
   ```

2. **Verify the URL is being received:**
   - With `--verbose`, you should see log messages when requests are received
   - Check for: "Received POST request from..." and "Extracted URL: ..."

3. **Check authentication:**
   - If you have a secret configured, ensure the `X-Auth` header is included and correct
   - Look for "Authentication successful" or "Unauthorized request" in verbose logs

4. **Verify Tidal credentials:**
   - The listener uses your saved Tidal authentication
   - If credentials expired, run: `riptide auth login`

5. **Check the queue:**
   - With `--verbose`, you'll see: "Added task to queue. Queue size: X"
   - And: "Processing download: <url>"

### Common Error Messages

**"Listener secret is not configured"**
- Warning only - authentication is disabled
- The listener will still work without a secret

**"Invalid URL: ..."**
- The URL format is not recognized
- Ensure it matches one of the supported formats (see above)

**"Unauthorized request from ..."**
- The X-Auth header is missing or doesn't match the configured secret
- Check your secret in `config.toml`

**"API Error: ..."**
- Problem communicating with Tidal
- Check your internet connection
- Try refreshing authentication: `riptide auth login`

### Debug Mode

Enable verbose logging to see detailed information about each step:

```bash
riptide listen --verbose
```

**Example verbose output:**
```
[cyan]Verbose logging enabled[/]
DEBUG:riptide:Debug logging is active
[green]Starting listener on 127.0.0.1:8123[/]
[blue]Send POST requests to http://127.0.0.1:8123/download[/]
[yellow]Authentication disabled: No X-Auth header required[/]
INFO:riptide:Download queue worker started
DEBUG:riptide:Worker thread started with ID: 123456789
DEBUG:riptide:Starting Flask server
 * Serving Flask app 'listen'
 * Running on http://127.0.0.1:8123
INFO:riptide:Received POST request from 127.0.0.1
INFO:riptide:Extracted URL: https://tidal.com/browse/track/123456
INFO:riptide:Parsed resource - type: track, id: 123456
INFO:riptide:Added to queue: https://tidal.com/browse/track/123456
INFO:riptide:Processing download: https://tidal.com/browse/track/123456
INFO:riptide:Processing resource type: track
INFO:riptide:Track fetched: Example Song by Example Artist
INFO:riptide:Starting track download: Artist/Album/Song.flac
INFO:riptide:Completed download: https://tidal.com/browse/track/123456
```

This will show (in the console):
- Request receipt and parsing
- Authentication checks
- URL parsing and resource type detection
- Queue operations (add, process, remove)
- Download progress
- API calls and responses
- Metadata operations
- File path generation

### Log Files

All riptide operations (including the listener) are logged to a file, regardless of verbose mode:

**Log file location:**
- **Linux/macOS:** `~/.riptide/latest.log`
- **Windows:** `C:\Users\<username>\.riptide\latest.log`

The log file is overwritten each time riptide starts. Check this file if:
- You're not seeing expected output in the console
- You need to review what happened after the fact
- The listener is running without verbose mode

**Example of checking logs:**
```bash
# View the log file
tail -f ~/.riptide/latest.log

# Or on another terminal while listener is running
cat ~/.riptide/latest.log
```

### Queue Behavior

- Resources (albums, playlists, etc.) are processed **sequentially** (one at a time)
- Within each resource, tracks are downloaded based on `concurrent_downloads` setting:
  - `concurrent_downloads = 1`: Tracks download one at a time
  - `concurrent_downloads > 1`: Multiple tracks download in parallel
- If a URL is already in the queue, duplicate submissions return "Already in queue"
- The queue continues processing even if one download fails
- Failed downloads are logged but don't stop the queue

### File Locations

Downloads are saved according to your `config.toml` settings:

```toml
[download]
download_path = "/path/to/your/music"
templates.track = "{album.artist}/{album.title}/{item.title}"
```

Check the verbose logs to see the exact file paths being generated.

## Integration Examples

### Browser Extension

Create a browser extension that captures Tidal URLs and sends them to the listener:

```javascript
// background.js
chrome.contextMenus.create({
  id: "download-tidal",
  title: "Download with riptide",
  contexts: ["link"],
  targetUrlPatterns: ["*://tidal.com/browse/*", "*://listen.tidal.com/browse/*"]
});

chrome.contextMenus.onClicked.addListener((info, tab) => {
  if (info.menuItemId === "download-tidal") {
    fetch('http://127.0.0.1:8123/download', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        'X-Auth': 'your-secret'
      },
      body: JSON.stringify({ url: info.linkUrl })
    }).then(response => {
      if (response.ok) {
        chrome.notifications.create({
          type: 'basic',
          iconUrl: 'icon.png',
          title: 'riptide',
          message: 'Download queued successfully'
        });
      }
    });
  }
});
```

### Shell Script

Create a script to quickly submit downloads from the command line:

```bash
#!/bin/bash
# save as: ~/bin/tidal-dl

URL="$1"
SECRET="your-secret-here"

if [ -z "$URL" ]; then
  echo "Usage: tidal-dl <url>"
  exit 1
fi

curl -X POST http://127.0.0.1:8123/download \
  -H "Content-Type: application/json" \
  -H "X-Auth: $SECRET" \
  -d "{\"url\": \"$URL\"}" \
  && echo "Download queued!"
```

Make it executable:
```bash
chmod +x ~/bin/tidal-dl
```

Usage:
```bash
tidal-dl https://tidal.com/browse/track/123456
```

## Development Notes

- The listener uses the same download logic as `riptide download url`
- All configuration settings (quality, metadata, paths, etc.) are respected
- Downloads use the configured number of threads from `download.threads_count`
- The Flask development server is used (suitable for local use only)

## FAQ

**Q: Can I access the listener from another computer?**  
A: No, it only binds to `127.0.0.1` (localhost) for security. If you need remote access, consider using SSH port forwarding or a reverse proxy with proper authentication.

**Q: How many downloads can be queued?**  
A: There's no hard limit on the queue size. Resources are processed one at a time from the queue.

**Q: What's the difference between sequential and concurrent track downloads?**  
A: With `concurrent_downloads = 1`, tracks within an album/playlist download one at a time. With higher values (e.g., 3), up to 3 tracks download simultaneously.

**Q: What happens if I restart the listener?**  
A: The queue is not persisted. Any pending downloads will be lost. Currently processing download will be interrupted.

**Q: Can I submit the same URL multiple times?**  
A: Duplicate URLs are rejected while they're in the queue. Once a download completes (successfully or with error), the URL can be submitted again.

**Q: How do concurrent downloads work with albums?**  
A: Albums are processed one at a time from the queue. When downloading an album, `concurrent_downloads` controls how many tracks are downloaded in parallel. For example, with `concurrent_downloads = 3`, up to 3 tracks from that album will download simultaneously.

**Q: Does the listener support all the same features as the CLI?**  
A: Yes, it uses the same download logic and respects all configuration settings including quality, metadata, templates, etc.