/**
 * @module k:testing/dom
 *
 * A minimal in-memory DOM for testing the runtime inside V8, where no
 * browser DOM exists (docs/compiler/testing.md, "Runtime JS tests"). It
 * implements exactly what the runtime and generated code use: the node
 * tree, attributes, inline style custom properties, `<template>` with an
 * `innerHTML` parser for compiler-generated HTML, events with capture-free
 * bubbling, and HTML serialization. It is not a browser: no layout, no
 * CSS, no form semantics beyond `value` and `checked` properties.
 *
 * `install()` puts a fresh `document` (and the classes) on `globalThis`.
 */

const VOID_ELEMENTS = new Set(["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"]);

/**
 * The base of every node.
 *
 * Children are a doubly linked list, as in browsers, so `firstChild`,
 * `nextSibling`, `insertBefore` and `removeChild` are O(1) and the
 * runtime's benchmarks (keyed lists of 10,000 rows, hydration) measure the
 * runtime rather than this test DOM (docs/compiler/performance.md §11).
 * `childNodes` is an array snapshot, rebuilt after a mutation.
 */
export class Node {
  constructor(nodeType, ownerDocument) {
    /** 1 element, 3 text, 8 comment, 9 document, 11 fragment. */
    this.nodeType = nodeType;
    /** @type {Document|null} */
    this.ownerDocument = ownerDocument;
    /** @type {Node|null} */
    this.parentNode = null;
    /** @type {Node|null} */
    this._first = null;
    /** @type {Node|null} */
    this._last = null;
    /** @type {Node|null} */
    this._next = null;
    /** @type {Node|null} */
    this._prev = null;
    /** The `childNodes` snapshot, or `null` after a mutation. @type {Node[]|null} */
    this._kids = null;
  }

  /**
   * The children in order, as an array snapshot (not live: a mutation
   * makes a new one).
   * @returns {Node[]}
   */
  get childNodes() {
    if (this._kids === null) {
      const kids = [];
      for (let c = this._first; c !== null; c = c._next) kids.push(c);
      this._kids = kids;
    }
    return this._kids;
  }

  /** @returns {Node|null} */
  get firstChild() {
    return this._first;
  }

  /** @returns {Node|null} */
  get lastChild() {
    return this._last;
  }

  /** @returns {Node|null} */
  get nextSibling() {
    return this._next;
  }

  /** @returns {Node|null} */
  get previousSibling() {
    return this._prev;
  }

  /** Appends `child` (or a fragment's children) and returns it. */
  appendChild(child) {
    return this.insertBefore(child, null);
  }

  /** Inserts `child` before `reference` (`null` appends) and returns it. */
  insertBefore(child, reference) {
    if (child.nodeType === 11) {
      while (child._first !== null) this.insertBefore(child._first, reference);
      return child;
    }
    if (child.parentNode !== null) child.parentNode.removeChild(child);
    if (reference !== null && reference.parentNode !== this) {
      throw new Error("insertBefore: the reference node is not a child");
    }
    const prev = reference === null ? this._last : reference._prev;
    child._prev = prev;
    child._next = reference;
    if (prev === null) this._first = child;
    else prev._next = child;
    if (reference === null) this._last = child;
    else reference._prev = child;
    child.parentNode = this;
    this._kids = null;
    return child;
  }

  /** Removes `child` and returns it. */
  removeChild(child) {
    if (child.parentNode !== this) throw new Error("removeChild: not a child");
    if (child._prev === null) this._first = child._next;
    else child._prev._next = child._next;
    if (child._next === null) this._last = child._prev;
    else child._next._prev = child._prev;
    child._prev = null;
    child._next = null;
    child.parentNode = null;
    this._kids = null;
    return child;
  }

  /** Detaches every child. */
  _clear() {
    for (let c = this._first; c !== null; ) {
      const next = c._next;
      c.parentNode = null;
      c._prev = null;
      c._next = null;
      c = next;
    }
    this._first = null;
    this._last = null;
    this._kids = null;
  }

  /** Removes this node from its parent, if any. */
  remove() {
    if (this.parentNode !== null) this.parentNode.removeChild(this);
  }

  /** The concatenated text of the subtree; setting replaces the children. */
  get textContent() {
    if (this.nodeType === 3 || this.nodeType === 8) return this.data;
    return this.childNodes.map((c) => (c.nodeType === 8 ? "" : c.textContent)).join("");
  }

  set textContent(value) {
    if (this.nodeType === 3 || this.nodeType === 8) {
      this.data = value;
      return;
    }
    this._clear();
    if (value !== "") this.appendChild(this.ownerDocument.createTextNode(value));
  }

  /** Copies the node, and its subtree when `deep`. */
  cloneNode(deep) {
    const copy = this.shallowClone();
    if (deep) for (let c = this._first; c !== null; c = c._next) copy.appendChild(c.cloneNode(true));
    return copy;
  }
}

/** A text node. */
export class Text extends Node {
  constructor(data, ownerDocument) {
    super(3, ownerDocument);
    this.data = data;
  }

  shallowClone() {
    return new Text(this.data, this.ownerDocument);
  }
}

/** A comment node (hydration markers). */
export class Comment extends Node {
  constructor(data, ownerDocument) {
    super(8, ownerDocument);
    this.data = data;
  }

  shallowClone() {
    return new Comment(this.data, this.ownerDocument);
  }
}

/** A document fragment. */
export class DocumentFragment extends Node {
  constructor(ownerDocument) {
    super(11, ownerDocument);
  }

  shallowClone() {
    return new DocumentFragment(this.ownerDocument);
  }
}

/** An event, dispatched with bubbling. */
export class Event {
  /** @param {string} type @param {{bubbles?: boolean}} [init] */
  constructor(type, init) {
    this.type = type;
    this.bubbles = init?.bubbles ?? false;
    this.target = null;
    this.currentTarget = null;
    this.defaultPrevented = false;
    this.propagationStopped = false;
  }

  preventDefault() {
    this.defaultPrevented = true;
  }

  stopPropagation() {
    this.propagationStopped = true;
  }
}

class Style {
  constructor() {
    /** @type {Map<string, string>} */
    this.properties = new Map();
  }

  setProperty(name, value) {
    this.properties.set(name, String(value));
  }

  removeProperty(name) {
    this.properties.delete(name);
  }

  getPropertyValue(name) {
    return this.properties.get(name) ?? "";
  }

  get cssText() {
    return [...this.properties].map(([k, v]) => `${k}: ${v};`).join(" ");
  }
}

/** An element. */
export class Element extends Node {
  constructor(tagName, ownerDocument) {
    super(1, ownerDocument);
    /** Lower case, as written by the compiler. */
    this.localName = tagName;
    /** @type {Map<string, string>} */
    this.attributes = new Map();
    this.style = new Style();
    /** @type {Map<string, Array<(e: Event) => void>>} */
    this.listeners = new Map();
    /** Present on `<template>` only. @type {DocumentFragment|null} */
    this.content = tagName === "template" ? new DocumentFragment(ownerDocument) : null;
  }

  get tagName() {
    return this.localName.toUpperCase();
  }

  get id() {
    return this.attributes.get("id") ?? "";
  }

  getAttribute(name) {
    return this.attributes.get(name) ?? null;
  }

  setAttribute(name, value) {
    this.attributes.set(name, String(value));
  }

  removeAttribute(name) {
    this.attributes.delete(name);
  }

  hasAttribute(name) {
    return this.attributes.has(name);
  }

  addEventListener(type, listener) {
    const list = this.listeners.get(type) ?? [];
    list.push(listener);
    this.listeners.set(type, list);
  }

  removeEventListener(type, listener) {
    const list = this.listeners.get(type);
    if (list === undefined) return;
    const i = list.indexOf(listener);
    if (i >= 0) list.splice(i, 1);
  }

  /** Dispatches `event` here and, if it bubbles, on each ancestor element. */
  dispatchEvent(event) {
    event.target = this;
    for (let n = this; n !== null && n.nodeType === 1; n = n.parentNode) {
      event.currentTarget = n;
      for (const listener of [...(n.listeners.get(event.type) ?? [])]) listener.call(n, event);
      if (!event.bubbles || event.propagationStopped) break;
    }
    return !event.defaultPrevented;
  }

  /** Dispatches a bubbling `click`. */
  click() {
    this.dispatchEvent(new Event("click", { bubbles: true }));
  }

  /** The serialized children; setting parses HTML. */
  get innerHTML() {
    const children = this.content !== null ? this.content.childNodes : this.childNodes;
    return children.map(serialize).join("");
  }

  set innerHTML(html) {
    const target = this.content !== null ? this.content : this;
    target._clear();
    parseInto(target, html, this.ownerDocument);
  }

  get outerHTML() {
    return serialize(this);
  }

  shallowClone() {
    const copy = new Element(this.localName, this.ownerDocument);
    for (const [k, v] of this.attributes) copy.attributes.set(k, v);
    for (const [k, v] of this.style.properties) copy.style.properties.set(k, v);
    if (this.content !== null) copy.content = this.content.cloneNode(true);
    return copy;
  }
}

/** The document. */
export class Document extends Node {
  constructor() {
    super(9, null);
    this.ownerDocument = this;
    this.documentElement = this.createElement("html");
    this.head = this.createElement("head");
    this.body = this.createElement("body");
    this.documentElement.appendChild(this.head);
    this.documentElement.appendChild(this.body);
    this.appendChild(this.documentElement);
  }

  createElement(tagName) {
    return new Element(tagName.toLowerCase(), this);
  }

  createTextNode(data) {
    return new Text(data, this);
  }

  createComment(data) {
    return new Comment(data, this);
  }

  createDocumentFragment() {
    return new DocumentFragment(this);
  }

  /** The first element in tree order with `id`, or `null`. */
  getElementById(id) {
    const visit = (n) => {
      if (n.nodeType === 1 && n.attributes.get("id") === id) return n;
      for (const c of n.childNodes) {
        const found = visit(c);
        if (found !== null) return found;
      }
      return null;
    };
    return visit(this);
  }

  shallowClone() {
    throw new Error("cloning a document is not supported");
  }
}

function escapeText(s) {
  return s.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

function escapeAttribute(s) {
  return escapeText(s).replaceAll('"', "&quot;");
}

/** Serializes a node as HTML, attributes in insertion order. */
export function serialize(node) {
  switch (node.nodeType) {
    case 3:
      return escapeText(node.data);
    case 8:
      return `<!--${node.data}-->`;
    case 11:
      return node.childNodes.map(serialize).join("");
    case 1: {
      let s = `<${node.localName}`;
      for (const [k, v] of node.attributes) s += v === "" ? ` ${k}` : ` ${k}="${escapeAttribute(v)}"`;
      if (node.style.properties.size > 0) s += ` style="${escapeAttribute(node.style.cssText)}"`;
      s += ">";
      if (VOID_ELEMENTS.has(node.localName)) return s;
      return `${s}${node.innerHTML}</${node.localName}>`;
    }
    default:
      return node.childNodes.map(serialize).join("");
  }
}

const ENTITIES = { amp: "&", lt: "<", gt: ">", quot: '"', "#39": "'", apos: "'", nbsp: " " };

function decode(s) {
  let out = "";
  let i = 0;
  while (i < s.length) {
    const amp = s.indexOf("&", i);
    if (amp < 0) {
      out += s.slice(i);
      break;
    }
    out += s.slice(i, amp);
    const semi = s.indexOf(";", amp);
    const name = semi < 0 ? null : s.slice(amp + 1, semi);
    if (name !== null && Object.hasOwn(ENTITIES, name)) {
      out += ENTITIES[name];
      i = semi + 1;
    } else if (name !== null && name.startsWith("#x")) {
      out += String.fromCodePoint(parseInt(name.slice(2), 16));
      i = semi + 1;
    } else if (name !== null && name.startsWith("#")) {
      out += String.fromCodePoint(parseInt(name.slice(1), 10));
      i = semi + 1;
    } else {
      out += "&";
      i = amp + 1;
    }
  }
  return out;
}

/**
 * Parses well-formed, compiler-style HTML (quoted attributes, explicit
 * closing tags except for void elements, comments) into `parent`.
 * Malformed input throws, so a test sees the generator's mistake.
 */
export function parseInto(parent, html, doc) {
  const stack = [parent];
  let i = 0;
  const top = () => stack[stack.length - 1];
  const sink = () => top().content ?? top();
  while (i < html.length) {
    if (html.startsWith("<!--", i)) {
      const end = html.indexOf("-->", i + 4);
      if (end < 0) throw new Error("unterminated comment");
      sink().appendChild(doc.createComment(html.slice(i + 4, end)));
      i = end + 3;
    } else if (html.startsWith("</", i)) {
      const end = html.indexOf(">", i);
      const name = html.slice(i + 2, end).trim().toLowerCase();
      const open = top();
      if (open === parent || open.localName !== name) throw new Error(`unexpected </${name}>`);
      stack.pop();
      i = end + 1;
    } else if (html[i] === "<") {
      let j = i + 1;
      while (j < html.length && !" \t\n/>".includes(html[j])) j++;
      const element = doc.createElement(html.slice(i + 1, j));
      for (;;) {
        while (" \t\n".includes(html[j])) j++;
        if (html[j] === ">" || html.startsWith("/>", j)) break;
        if (j >= html.length) throw new Error("unterminated tag");
        let k = j;
        while (k < html.length && !" \t\n=/>".includes(html[k])) k++;
        const name = html.slice(j, k);
        if (html[k] === "=") {
          const quote = html[k + 1];
          if (quote !== '"' && quote !== "'") throw new Error(`unquoted attribute ${name}`);
          const close = html.indexOf(quote, k + 2);
          element.setAttribute(name, decode(html.slice(k + 2, close)));
          j = close + 1;
        } else {
          element.setAttribute(name, "");
          j = k;
        }
      }
      const selfClosing = html.startsWith("/>", j);
      i = j + (selfClosing ? 2 : 1);
      sink().appendChild(element);
      if (!selfClosing && !VOID_ELEMENTS.has(element.localName)) stack.push(element);
    } else {
      const next = html.indexOf("<", i);
      const end = next < 0 ? html.length : next;
      sink().appendChild(doc.createTextNode(decode(html.slice(i, end))));
      i = end;
    }
  }
  if (stack.length !== 1) throw new Error(`unclosed <${top().localName}>`);
}

/**
 * Installs a fresh document and the DOM classes on `globalThis` and
 * returns the document.
 * @returns {Document}
 */
export function install() {
  const doc = new Document();
  Object.assign(globalThis, { document: doc, Node, Element, Text, Comment, DocumentFragment, Event });
  return doc;
}
