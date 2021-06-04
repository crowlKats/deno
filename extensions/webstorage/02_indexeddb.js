// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

"use strict";

((window) => {
  const core = window.Deno.core;
  const webidl = window.__bootstrap.webidl;

  function valueToKey(input, seen = []) {
    if (seen.includes(input)) {
      return null;
    }

    if (typeof input === "number") {
      if (isNaN(input)) {
        return null;
      } else {
        return {
          type: "number",
          value: input,
        };
      }
    } else if (input instanceof Date) {
      const ms = input.getTime();
      if (isNaN(ms)) {
        return null;
      } else {
        return {
          type: "date",
          value: ms,
        };
      }
    } else if (typeof input === "string") {
      return {
        type: "string",
        value: input,
      };
    } else if (input instanceof ArrayBuffer || ArrayBuffer.isView(input)) {
      const bytes = webidl.converters.BufferSource(input).slice();
      return {
        type: "binary",
        value: bytes,
      };
    } else if (Array.isArray(input)) {
      // TODO: check
      seen.push(input);
      const keys = [];
      for (const entry of input) {
        const key = valueToKey(entry, seen);
        if (key === null) {
          return null;
        }
        keys.push(key);
      }
      return keys;
    } else {
      return null;
    }
  }
  function compareKeys(a, b) {
    const ta = a.type;
    const tb = b.type;

    if (ta !== tb) {
      if (ta === "array") {
        return 1;
      } else if (tb === "array") {
        return -1;
      } else if (ta === "binary") {
        return 1;
      } else if (tb === "binary") {
        return -1;
      } else if (ta === "string") {
        return 1;
      } else if (tb === "string") {
        return -1;
      } else if (ta === "date") {
        return 1;
      } else {
        // TODO: assert
        return -1;
      }
    }

    const va = a.value;
    const vb = b.value;

    switch (ta) {
      case "number":
      case "date":
        if (va > vb) {
          return 1;
        } else if (va < vb) {
          return -1;
        } else {
          return 0;
        }

      case "string":
        // TODO: check
        if (va < vb) {
          return -1;
        } else if (va > vb) {
          return 1;
        } else {
          return 0;
        }

      case "binary":
        // TODO
        break;

      case "array": {
        const length = Math.min(va.length, vb.length);
        for (let i = 0; i < length; i++) {
          const c = compareKeys(va[i], vb[i]);
          if (c !== 0) {
            return c;
          }
        }
        if (va.length > vb.length) {
          return 1;
        } else if (va.length < vb.length) {
          return -1;
        } else {
          return 0;
        }
      }
    }
  }

  class IDBFactory {
    open(name, version) {
      const prefix = "Failed to execute 'open' on 'IDBFactory'";
      webidl.requiredArguments(arguments.length, 1, { prefix });
      name = webidl.converters.DOMString(name, {
        prefix,
        context: "Argument 1",
      });
      version = version === undefined
        ? undefined
        : webidl.converters["unsigned long long"](version, {
          prefix,
          context: "Argument 2",
        });

      if (version === 0) {
        throw new TypeError(); // TODO: message
      }

      // TODO

      const request = webidl.createBranded(IDBOpenDBRequest);

      core.opAsync("op_indexeddb_open_database", name, version).then(
        ([rid, upgraded]) => {
          if (upgraded) {
            request.dispatchEvent();
          }

          request.dispatchEvent();
        },
      ).catch((e) => {
        request[_result] = undefined;
        request[_error] = e;
        request[_done] = true;
        const errEvent = new ErrorEvent("error", {
          bubbles: true,
          cancelable: true,
        });
        errEvent.target = request;
        request.dispatchEvent(errEvent);
      });

      return request;
    }

    deleteDatabase(name) {
      const prefix = "Failed to execute 'deleteDatabase' on 'IDBFactory'";
      webidl.requiredArguments(arguments.length, 1, { prefix });
      name = webidl.converters.DOMString(name, {
        prefix,
        context: "Argument 1",
      });

      // TODO
    }

    databases() {
      return core.opAsync("op_indexeddb_databases");
    }

    cmp(first, second) {
      webidl.requiredArguments(arguments.length, 2, {
        prefix: "Failed to execute 'cmp' on 'IDBFactory'",
      });

      const a = valueToKey(first);
      if (a === null) {
        throw new DOMException("Invalid type", "DataError");
      }
      const b = valueToKey(second);
      if (b === null) {
        throw new DOMException("Invalid type", "DataError");
      }

      return compareKeys(a, b);
    }
  }

  const _processed = Symbol("[[processed]]");
  const _result = Symbol("[[result]]");
  const _error = Symbol("[[error]]");
  const _done = Symbol("[[done]]");
  const _source = Symbol("[[source]]");
  const _transaction = Symbol("[[transaction]]");
  class IDBRequest extends EventTarget {
    [_processed] = false;
    [_done] = false;
    [_source];
    [_result];
    [_error];
    [_transaction] = null;

    constructor() {
      super();
      webidl.illegalConstructor();
    }
  }

  class IDBOpenDBRequest extends IDBRequest {
    constructor() {
      super();
      webidl.illegalConstructor();
    }
  }

  window.__bootstrap.webgpu = {
    IDBFactory,
    IDBRequest,
    IDBOpenDBRequest,
  };
})(this);
