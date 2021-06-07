// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

"use strict";

((window) => {
  const core = window.Deno.core;
  const webidl = window.__bootstrap.webidl;

  const _stream = Symbol("[[stream]]");

  class CompressionStream {
    [_stream];

    constructor(format) {
      const prefix = "Failed to construct 'CompressionStream'";
      webidl.requiredArguments(arguments.length, 1, { prefix });
      format = webidl.converters.DOMString(format, {
        prefix,
        context: "Argument 1",
      });

      if (format !== "deflate" && format !== "gzip") {
        throw new TypeError(`Format '${format}' not supported`);
      }

      const rid = core.opSync("op_create_compressor", format);

      this[_stream] = new TransformStream({
        transform(chunk, controller) {
          chunk = webidl.converters.BufferSource(chunk);
          const buffer = core.opSync("op_compress", rid, new Uint8Array(chunk));
          if (buffer.length === 0) {
            return;
          }
          controller.enqueue(buffer);
        },
        flush(controller) {
          // TODO
        },
      });
    }

    get readable() {
      return this[_stream].readable;
    }

    get writable() {
      return this[_stream].writable;
    }

    [Symbol.for("Deno.customInspect")](inspect) {
      return `${this.constructor.name} ${
        inspect({
          readable: this.readable,
          writable: this.writable,
        })
      }`;
    }
  }

  class DecompressionStream {
    [_stream];

    constructor(format) {
      const prefix = "Failed to construct 'DecompressionStream'";
      webidl.requiredArguments(arguments.length, 1, { prefix });
      format = webidl.converters.DOMString(format, {
        prefix,
        context: "Argument 1",
      });

      if (format !== "deflate" && format !== "gzip") {
        throw new TypeError(`Format '${format}' not supported`);
      }

      const rid = core.opSync("op_create_decompressor", format);

      this[_stream] = new TransformStream({
        transform(chunk, controller) {
          chunk = webidl.converters.BufferSource(chunk);
          const buffer = core.opSync("op_compress", rid, new Uint8Array(chunk));
          if (buffer.length === 0) {
            return;
          }
          controller.enqueue(buffer);
        },
        flush(controller) {
          // TODO
        },
      });
    }

    get readable() {
      return this[_stream].readable;
    }

    get writable() {
      return this[_stream].writable;
    }

    [Symbol.for("Deno.customInspect")](inspect) {
      return `${this.constructor.name} ${
        inspect({
          readable: this.readable,
          writable: this.writable,
        })
      }`;
    }
  }

  window.__bootstrap.compression = {
    CompressionStream,
    DecompressionStream,
  };
})(this);
