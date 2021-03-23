// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

((window) => {
  const core = window.Deno.core;
  const webidl = window.__bootstrap.webidl;

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

  const availablePorts = [];

  class WindowSerial extends EventTarget {
    async getPorts() {
      // TODO check perms

      return availablePorts;
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
      availablePorts.push(port);
      return port;
    }
  }

  class WorkerSerial extends EventTarget {
    async getPorts() {
      // TODO check perms

      return availablePorts;
    }
  }

  const _state = Symbol("[[state]]");
  const _bufferSize = Symbol("[[bufferSize]]");
  const _readable = Symbol("[[readable]]");
  const _readFatal = Symbol("[[readFatal]]");
  const _writable = Symbol("[[writable]]");
  const _writeFatal = Symbol("[[writeFatal]]");
  const _pendingClosePromise = Symbol("[[pendingClosePromise]]");


  class SerialPort extends EventTarget {
    #rid;
    #device;
    [_state] = "closed";
    [_bufferSize];
    [_readable] = null;
    [_readFatal] = false;
    [_writable] = null;
    [_writeFatal] = false;
    [_pendingClosePromise] = null;

    get readable() {
      if (this[_readable] !== null) {
        return this[_readable];
      }
      if (this[_state] !== "opened" || this[_readFatal]) {
        return null;
      }

      this[_readable] = new ReadableStream({
        async pull(controller) {
          const buffer = new Uint8Array(controller.desiredSize);
          await core.jsonOpAsync("op_webserial_read", {
            rid: this.#rid,
          }, buffer);
          controller.enqueue(buffer);
          // TODO: error handling
        },
        async cancel() {
          // TODO
          this[_readable] = null;
          if (this[_writable] === null && this[_pendingClosePromise] !== null) {
            this[_pendingClosePromise].resolve();
          }
        },
      }, {
        highWaterMark: this[_bufferSize],
        size(chunk) {
          // TODO
        },
      });

      return this[_readable];
    }

    get writable() {
      if (this[_writable] !== null) {
        return this[_writable];
      }
      if (this[_state] !== "opened" || this[_writeFatal]) {
        return null;
      }

      this[_writable] = new WritableStream({
        async write(chunk) {
          const bytes = webidl.converters.BufferSource(chunk).slice();
          await core.jsonOpAsync("op_webserial_write", {
            rid: this.#rid,
          }, bytes);
          // TODO: error handling
        },
        async abort() {
          // TODO
          this[_writable] = null;
          if (this[_readable] === null && this[_pendingClosePromise] !== null) {
            this[_pendingClosePromise].resolve();
          }
        },
        async close() {
          // TODO
          this[_writable] = null;
          if (this[_readable] === null && this[_pendingClosePromise] !== null) {
            this[_pendingClosePromise].resolve();
          }
        },
      }, {
        highWaterMark: this[_bufferSize],
        size(chunk) {
          // TODO
        },
      });

      return this[_writable];
    }

    constructor(device) {
      super();

      this.#device = device;
    }

    getInfo() {}

    async open(options) {
      if (this[_state] !== "closed") {
        throw new DOMException("", "InvalidStateError");
      }

      this[_bufferSize] = options.bufferSize ?? 255;
      this[_state] = "opening";
      this.#rid = core.jsonOpSync("op_webserial_open", {
        device: this.#device,
        ...options,
      });

      this[_state] = "opened";
    }

    async setSignals(signals) {
      if (this[_state] !== "opened") {
        throw new DOMException("", "InvalidStateError");
      }

      core.jsonOpSync("op_webserial_set_signals", {
        rid: this.#rid,
        ...signals,
      });
    }

    async getSignals() {
      if (this[_state] !== "opened") {
        throw new DOMException("", "InvalidStateError");
      }

      return core.jsonOpSync("op_webserial_set_signals", { rid: this.#rid });
    }

    async close() {
      let cancelPromise;
      if (this[_readable] === null) {
        cancelPromise = new Promise(res => res());
      } else {
        cancelPromise = this[_readable].cancel();
      }

      let abortPromise;
      if (this[_writable] === null) {
        abortPromise = new Promise(res => res());
      } else {
        abortPromise = this[_writable].abort();
      }

      this[_pendingClosePromise] = createResolvable();
      if (this[_readable] === null && this[_writable] === null) {
        this[_pendingClosePromise].resolve();
      }

      const combinedPromise = Promise.all([cancelPromise, abortPromise, this[_pendingClosePromise]]);

      this[_state] = "closing";

      try {
        await combinedPromise;
        core.close(this.#rid);
        this[_state] = "closed";
        this[_readFatal] = false;
        this[_writeFatal] = false;
        this[_pendingClosePromise] = null;
      } catch (e) {
        this[_pendingClosePromise] = null;
        throw e;
      }
    }
  }

  window.__bootstrap.webSerial = {
    windowSerial: new WindowSerial(),
    workerSerial: new WorkerSerial(),
  };
})(this);
