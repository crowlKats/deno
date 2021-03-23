// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

pub fn init(rt: &mut deno_core::JsRuntime) {
  super::reg_json_sync(
    rt,
    "op_webserial_open",
    deno_webserial::op_webserial_open,
  );
  super::reg_json_async(
    rt,
    "op_webserial_read",
    deno_webserial::op_webserial_read,
  );
  super::reg_json_async(
    rt,
    "op_webserial_write",
    deno_webserial::op_webserial_write,
  );
  super::reg_json_sync(
    rt,
    "op_webserial_set_signals",
    deno_webserial::op_webserial_set_signals,
  );
  super::reg_json_sync(
    rt,
    "op_webserial_get_signals",
    deno_webserial::op_webserial_get_signals,
  );
}
