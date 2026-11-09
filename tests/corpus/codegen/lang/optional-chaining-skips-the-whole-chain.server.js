// file: k:/app/corpus.js
import * as $k from "k:runtime";

export function AccountId(value) {
  return { value };
}

export function AccountId$name(self) {
  return `account ${self.value}`;
}

export function User(id, tags) {
  return { id, tags };
}

export function Session(user) {
  return { user };
}

export function nameOf(session) {
  let $o1;
  return ($o1 = session.user) == null ? null : AccountId$name($o1.id);
}

export function valueOf(session) {
  return session.user?.id.value ?? null;
}

export function firstTag(session) {
  let $o1;
  return ($o1 = session.user) == null ? null : $k.at($o1.tags, 0);
}
