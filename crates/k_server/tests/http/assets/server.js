// Static assets are served by the Rust front from memory before any
// JavaScript runs (docs/compiler/server.md §13). This bundle answers every
// request that reaches it with `x-from-app: 1`, so a script can tell which
// requests the front served itself.
import { serve } from "k:runtime/server";

await serve(async (request) => ({
  status: 404,
  headers: [
    ["content-type", "text/plain; charset=utf-8"],
    ["x-from-app", "1"],
  ],
  body: `app saw ${request.method} ${request.path}`,
}));
