// Request forgery checks of the HTTP front with an https public origin
// (docs/design/26-auth-and-sessions.md, "Request forgery";
// docs/compiler/server.md §5). Every request that reaches JavaScript is
// answered 200 with `x-from-app: 1`.
import { serve } from "k:runtime/server";

await serve(async (request) => ({
  status: 200,
  headers: [
    ["content-type", "application/json"],
    ["x-from-app", "1"],
  ],
  body: JSON.stringify({ method: request.method, path: request.path }),
}));
