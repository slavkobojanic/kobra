// A hand-written server bundle that exercises the HTTP front and the
// worker request loop without the code generator: what generated server
// code will do, reduced to the protocol between the front and `serve()`.
import { Out, serve } from "k:runtime/server";

const html = (status, out, extra = []) => ({
  status,
  headers: [["content-type", "text/html; charset=utf-8"], ...extra],
  body: out.html(),
});

const json = (status, value, extra = []) => ({
  status,
  headers: [["content-type", "application/json"], ...extra],
  body: JSON.stringify(value),
});

const decoder = new TextDecoder();

await serve(async (request) => {
  const [path, query = ""] = request.path.split("?");
  if (request.method === "GET" && path === "/") {
    const pair = query.split("&").find((p) => p.startsWith("name="));
    const name = pair === undefined ? "world" : decodeURIComponent(pair.slice(5));
    const out = new Out();
    out.raw("<!doctype html><html><body><h1>");
    out.text(`Hello, ${name}`);
    out.raw("</h1></body></html>");
    return html(200, out);
  }
  if (path === "/fixture.GreeterProtocol/Greet") {
    const message = JSON.parse(decoder.decode(request.body));
    if (typeof message.name !== "string") {
      return json(400, { code: "invalid_argument", message: "name is required" });
    }
    return json(200, { value: `Hello, ${message.name}` });
  }
  if (path === "/fixture.GreeterProtocol/Fail") {
    throw new Error("a panic in a handler");
  }
  if (path === "/webhooks/echo" && request.method === "POST") {
    const signature = request.headers.find(([n]) => n === "x-signature")?.[1] ?? "";
    return { status: 200, headers: [["content-type", "application/octet-stream"], ["x-signature-seen", signature]], body: request.body };
  }
  if (path === "/sign-in" && request.method === "POST") {
    return { status: 204, headers: [["set-cookie", "k_session=abc123; HttpOnly; SameSite=Lax; Path=/"]], body: "" };
  }
  if (path === "/whoami") {
    const cookie = request.headers.find(([n]) => n === "cookie")?.[1] ?? "";
    return json(200, { cookie });
  }
  if (path === "/slow") {
    await new Promise((resolve) => queueMicrotask(resolve));
    return json(200, { slow: true });
  }
  return { status: 404, headers: [["content-type", "text/plain; charset=utf-8"]], body: "not found" };
});
