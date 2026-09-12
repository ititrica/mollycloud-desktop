const port = process.argv[2] || "9333";
const targets = await fetch(`http://127.0.0.1:${port}/json/list`).then((response) => response.json());

function inspect(target) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(target.webSocketDebuggerUrl);
    const timeout = setTimeout(() => {
      socket.close();
      reject(new Error(`Timed out while inspecting ${target.title}`));
    }, 5000);

    socket.addEventListener("open", () => {
      socket.send(JSON.stringify({
        id: 1,
        method: "Runtime.evaluate",
        params: {
          expression: `(async () => {
            const probeUrls = location.pathname.endsWith("overlay.html") ? [
              "/live2d/molly/seethrough_output.model3.json",
              "/live2d/molly/seethrough_output.moc3",
              "/live2d/molly/seethrough_output.4096/texture_00.png"
            ] : [];
            const probes = [];
            for (const url of probeUrls) {
              try {
                const response = await fetch(url);
                probes.push({ url, ok: response.ok, status: response.status, type: response.headers.get("content-type") });
              } catch (error) {
                probes.push({ url, error: String(error) });
              }
            }
            return {
              title: document.title,
              origin: location.origin,
              path: location.pathname,
              canvasCount: document.querySelectorAll("canvas").length,
              overlayError: document.querySelector(".overlay-error")?.textContent?.trim() || null,
              overlayStatus: document.querySelector(".overlay-status")?.textContent?.trim() || null,
              resources: performance.getEntriesByType("resource").map((entry) => entry.name),
              probes
            };
          })()`,
          awaitPromise: true,
          returnByValue: true,
        },
      }));
    });

    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (message.id !== 1) return;
      clearTimeout(timeout);
      socket.close();
      resolve(message.result?.result?.value ?? null);
    });
    socket.addEventListener("error", () => {
      clearTimeout(timeout);
      reject(new Error(`Could not inspect ${target.title}`));
    });
  });
}

const pages = targets.filter((target) => target.type === "page");
const results = [];
for (const page of pages) {
  results.push(await inspect(page));
}
console.log(JSON.stringify(results, null, 2));
