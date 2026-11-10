/*
  Tiny static file server for the frontend preview harness.
  No dependencies, no install step.

    node scripts/dev-server.js            # http://localhost:8123
    node scripts/dev-server.js 3000       # custom port

  Open http://localhost:<port>/preview/harness.html to work on src/styles.css and
  src/app.js without rebuilding the Rust side.
*/
const http = require("http");
const fs = require("fs");
const path = require("path");

const port = Number(process.argv[2] || process.env.PORT || 8123);
const root = path.resolve(__dirname, "..");

const types = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".svg": "image/svg+xml"
};

const server = http.createServer((request, response) => {
  let target = decodeURIComponent((request.url || "/").split("?")[0]);
  if (target === "/") target = "/preview/harness.html";

  const file = path.join(root, target);
  if (!file.startsWith(root)) {
    response.writeHead(403);
    response.end("forbidden");
    return;
  }

  fs.readFile(file, (error, data) => {
    if (error) {
      response.writeHead(404, { "content-type": "text/plain" });
      response.end("not found: " + target);
      return;
    }
    response.writeHead(200, {
      "content-type": types[path.extname(file).toLowerCase()] || "application/octet-stream",
      "cache-control": "no-store"
    });
    response.end(data);
  });
});

server.listen(port, "127.0.0.1", () => {
  console.log(`preview server: http://localhost:${port}/preview/harness.html`);
});
