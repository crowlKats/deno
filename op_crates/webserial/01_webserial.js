// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

((window) => {
  const core = window.Deno.core;

  function createResolvable() {
    let resolve;
    let reject;
    const promise = new Promise((res, rej) => {
      resolve = res;
      reject = rej;
    });
    promise.resolve = resolve;
    promise.reject = reject;
    return promise;
  }

  class WebSerial extends EventTarget {
    #availablePorts = [];

    async getPorts() {
      // TODO check perms

      return this.#availablePorts;
    }

    async requestPort(options) {
      // TODO check perms

      if (options.filters) {
        for (const filter of options.filters) {
          if (!("usbVendorId" in filter)) {
            throw new TypeError();
          }
        }
      }

      const device = undefined; // TODO pick port

      const port = new SerialPort(device);
      this.#availablePorts.push(port);
      return port;
    }
  }

  class SerialPort extends EventTarget {
    #rid;
    #device;
    #bufferSize;
    #pendingClosePromise = null;
    #state = "closed";
    #readFatal = false;
    #writeFatal = false;

    #readable = null;
    get readable() {
      if (this.#readable !== null) {
        return this.#readable;
      }
      if (this.#state !== "opened" || this.#readFatal) {
        return null;
      }

      this.#readable = new ReadableStream({
        async pull(controller) {
          const buffer = new Uint8Array(this.#bufferSize);
          core.jsonOpSync("op_webserial_read", {
            rid: this.#rid,
          }, buffer);
          controller.enqueue(buffer);
        },
        async cancel() {
          // TODO
          this.#readable = null;
          if (this.#writable === null && this.#pendingClosePromise !== null) {
            this.#pendingClosePromise.resolve();
          }
        },
      }, {
        highWaterMark: this.#bufferSize,
        size(chunk) {
          // TODO
        },
      });

      return this.#readable;
    }

    #writable = null;
    get writable() {
      if (this.#writable !== null) {
        return this.#writable;
      }
      if (this.#state !== "opened" || this.#writeFatal) {
        return null;
      }

      this.#writable = new WritableStream({
        async write(chunk) {
          core.jsonOpSync("op_webserial_write", {
            rid: this.#rid,
          }, chunk.slice());
        },
        async abort() {
          // TODO
          this.#writable = null;
          if (this.#readable === null && this.#pendingClosePromise !== null) {
            this.#pendingClosePromise.resolve();
          }
        },
        async close() {
          // TODO
          this.#writable = null;
          if (this.#readable === null && this.#pendingClosePromise !== null) {
            this.#pendingClosePromise.resolve();
          }
        },
      }, {
        highWaterMark: this.#bufferSize,
        size(chunk) {
          // TODO
        },
      });

      return this.#writable;
    }

    constructor(device) {
      super();

      this.#device = device;
    }

    getInfo() {}

    async open(options) {
      if (this.#state !== "closed") {
        throw new DOMException("", "InvalidStateError");
      }

      this.#bufferSize = options.bufferSize ?? 255;
      this.#state = "opening";
      this.#rid = core.jsonOpSync("op_webserial_open", {
        device: this.#device,
        ...options,
      });

      this.#state = "opened";
    }

    async setSignals(signals) {
      core.jsonOpSync("op_webserial_set_signals", {
        rid: this.#rid,
        ...signals,
      });
    }

    async getSignals() {
      return core.jsonOpSync("op_webserial_set_signals", { rid: this.#rid });
    }

    async close() {
      const read = this.#readable.cancel();
      const write = this.#writable.abort();

      this.#pendingClosePromise = createResolvable();
      if (this.#readable === null && this.#writable === null) {
        this.#pendingClosePromise.resolve();
      }

      const res = Promise.all([read, write, this.#pendingClosePromise]);

      this.#state = "closing";

      try {
        await res;
        this.#state = "closed";
        this.#readFatal = false;
        this.#writeFatal = false;
        this.#pendingClosePromise = null;
      } catch (e) {
        this.#pendingClosePromise = null;
        throw e;
      }

      core.close(this.#rid);
    }
  }

  window.__bootstrap.webSerial = { serial: new WebSerial() };
})(this);
