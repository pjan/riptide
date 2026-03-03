// Logging state - will be loaded from settings
let loggingEnabled = true;

// Load logging preference on startup
chrome.storage.sync.get({ enableLogging: true }, (items) => {
  loggingEnabled = items.enableLogging;
  if (loggingEnabled) {
    console.log("[URL Sender] Background script loaded");
    console.log("[URL Sender] Logging enabled");
  }
});

// Listen for settings changes
chrome.storage.onChanged.addListener((changes, area) => {
  if (area === "sync" && changes.enableLogging) {
    loggingEnabled = changes.enableLogging.newValue;
    if (loggingEnabled) {
      console.log("[URL Sender] Logging enabled");
    }
  }
});

// Debug logging function
function log(...args) {
  if (loggingEnabled) {
    console.log("[URL Sender]", ...args);
  }
}

// Show a badge on the extension icon
function showBadge(text, color) {
  chrome.action.setBadgeText({ text: text });
  chrome.action.setBadgeBackgroundColor({ color: color });

  // Clear badge after 2 seconds
  setTimeout(() => {
    chrome.action.setBadgeText({ text: "" });
  }, 2000);
}

// Listen for extension icon click
chrome.action.onClicked.addListener(async (tab) => {
  log("Extension icon clicked");
  log("Tab info:", { id: tab.id, url: tab.url, title: tab.title });

  // Get current tab URL
  const url = tab.url;

  if (!url) {
    log("ERROR: No URL found in current tab");
    showBadge("ERR", "#ff0000");
    return;
  }

  // Get settings from storage
  log("Loading settings from storage...");
  const settings = await chrome.storage.sync.get({
    serverAddress: "localhost",
    serverPort: "8080",
    secret: "",
    enableLogging: true,
  });

  // Update logging preference if it changed
  loggingEnabled = settings.enableLogging;

  log("Settings loaded:", {
    serverAddress: settings.serverAddress,
    serverPort: settings.serverPort,
    secretLength: settings.secret.length,
  });

  // Construct server URL
  const serverUrl = `http://${settings.serverAddress}:${settings.serverPort}/download`;
  log("Server URL:", serverUrl);

  // Prepare request data
  const requestBody = { url: url };
  log("Request body:", requestBody);
  log(
    "X-Auth header:",
    settings.secret ? `${settings.secret.substring(0, 3)}***` : "(empty)",
  );

  // Send POST request
  log("Sending POST request...");
  showBadge("...", "#ffaa00");

  try {
    const response = await fetch(serverUrl, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Auth": settings.secret,
      },
      body: JSON.stringify(requestBody),
    });

    log("Response received:", {
      status: response.status,
      statusText: response.statusText,
      ok: response.ok,
    });

    if (response.ok) {
      log("✓ URL sent successfully:", url);
      showBadge("✓", "#00aa00");

      // Try to read response body
      try {
        const responseText = await response.text();
        if (responseText) {
          log("Response body:", responseText);
        }
      } catch (e) {
        log("Could not read response body:", e);
      }
    } else {
      log(
        "✗ Failed to send URL. Status:",
        response.status,
        response.statusText,
      );
      showBadge("✗", "#ff0000");

      // Try to read error response
      try {
        const errorText = await response.text();
        log("Error response:", errorText);
      } catch (e) {
        log("Could not read error response:", e);
      }
    }
  } catch (error) {
    log("✗ Error sending URL:", error);
    log("Error details:", {
      name: error.name,
      message: error.message,
      stack: error.stack,
    });
    showBadge("✗", "#ff0000");
  }
});

log("Extension ready and listening for icon clicks");
