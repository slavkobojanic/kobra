// Handlers that return something other than a response object, or fail
// with something other than an Error. `serve()` must turn each into a 500
// and keep the worker's request loop running.
import { serve } from "k:runtime/server";

await serve(async (request) => {
  switch (request.path) {
    case "/undefined":
      return undefined;
    case "/not-an-object":
      return "just a string";
    case "/throw-string":
      throw "a thrown string with a SECRET-DETAIL";
    case "/reject":
      return Promise.reject(new Error("rejected with SECRET-DETAIL"));
    case "/alive":
      return { status: 200, headers: [["content-type", "text/plain; charset=utf-8"]], body: "still alive" };
    default:
      return { status: 404, headers: [], body: "" };
  }
});
