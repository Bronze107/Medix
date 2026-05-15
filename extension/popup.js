const DEFAULT_PORT = 8765;

async function checkHealth(port) {
  try {
    const resp = await fetch(`http://localhost:${port}/api/health`);
    return resp.ok;
  } catch {
    return false;
  }
}

async function updateStatus() {
  const result = await chrome.storage.local.get("port");
  const port = result.port || DEFAULT_PORT;
  document.getElementById("port").value = port;

  const online = await checkHealth(port);
  const dot = document.getElementById("status-dot");
  const text = document.getElementById("status-text");

  dot.className = online ? "dot online" : "dot offline";
  text.textContent = online ? "Medix — 已连接" : "Medix — 未连接";
}

document.getElementById("port").addEventListener("change", async (e) => {
  const port = parseInt(e.target.value) || DEFAULT_PORT;
  await chrome.storage.local.set({ port });
  updateStatus();
});

updateStatus();
setInterval(updateStatus, 3000);
