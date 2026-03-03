// Load saved settings when page opens
document.addEventListener("DOMContentLoaded", () => {
  chrome.storage.sync.get(
    {
      serverAddress: "127.0.0.1",
      serverPort: "8123",
      secret: "",
      enableLogging: false,
    },
    (items) => {
      document.getElementById("serverAddress").value = items.serverAddress;
      document.getElementById("serverPort").value = items.serverPort;
      document.getElementById("secret").value = items.secret;
      document.getElementById("enableLogging").checked = items.enableLogging;
    },
  );
});

// Save settings when button is clicked
document.getElementById("save").addEventListener("click", () => {
  const serverAddress = document.getElementById("serverAddress").value;
  const serverPort = document.getElementById("serverPort").value;
  const secret = document.getElementById("secret").value;
  const enableLogging = document.getElementById("enableLogging").checked;

  chrome.storage.sync.set(
    {
      serverAddress: serverAddress,
      serverPort: serverPort,
      secret: secret,
      enableLogging: enableLogging,
    },
    () => {
      // Show success message
      const status = document.getElementById("status");
      status.textContent = "Settings saved successfully!";
      status.className = "status success";
      status.style.display = "block";

      // Hide message after 3 seconds
      setTimeout(() => {
        status.style.display = "none";
      }, 3000);
    },
  );
});
