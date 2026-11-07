// What the front does with unusual responses from JavaScript and with
// request details it passes on (docs/compiler/server.md §4, §15): "never
// panic in Rust on input", invalid statuses are 500 and invalid headers
// are skipped. Handlers that break the response protocol are in
// ../broken-handlers.
import { serve } from "k:runtime/server";

const text = (status, body, extra = []) => ({
  status,
  headers: [["content-type", "text/plain; charset=utf-8"], ...extra],
  body,
});

await serve(async (request) => {
  const [path] = request.path.split("?");
  switch (path) {
    case "/echo-path":
    case "/echo-path/%3Cx%3E":
      return text(200, request.path);
    case "/echo-headers":
      return text(200, request.headers.filter(([n]) => n.startsWith("x-")).map(([n, v]) => `${n}=${v}`).join(";"));
    case "/echo-method":
      return text(200, request.method);
    case "/body-length":
      return text(200, String(request.body.length));
    case "/bytes":
      return { status: 200, headers: [["content-type", "application/octet-stream"]], body: new Uint8Array([104, 105]) };
    case "/status-99":
      return text(99, "too small");
    case "/status-1000":
      return text(1000, "too large");
    case "/status-70000":
      return text(70000, "not a u16");
    case "/bad-header-name":
      return text(200, "kept", [["bad header", "x"], ["x-good", "yes"]]);
    case "/bad-header-value":
      return text(200, "kept", [["x-bad", "line\nbreak"], ["x-good", "yes"]]);
    case "/empty":
      return { status: 204, headers: [], body: "" };
    case "/fixture.EdgeProtocol/Echo":
      return { status: 200, headers: [["content-type", "application/json"]], body: '{"value":"echo"}' };
    default:
      return text(404, "not found");
  }
});
