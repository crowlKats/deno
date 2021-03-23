// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

use deno_core::error::type_error;
use deno_core::error::AnyError;
use deno_core::error::{bad_resource_id, resource_unavailable};
use deno_core::serde_json::json;
use deno_core::serde_json::Value;
use deno_core::OpState;
use deno_core::Resource;
use deno_core::{serde_json, ZeroCopyBuf};
use deno_core::{AsyncRefCell, JsRuntime, RcRef};
use serde::Deserialize;
use std::borrow::Cow;
use std::io::{Read, Write};

struct WebSerialPortResource(AsyncRefCell<Box<dyn serialport::SerialPort>>);

impl Resource for WebSerialPortResource {
  fn name(&self) -> Cow<str> {
    "webSerialPort".into()
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenArgs {
  device: String,
  baud_rate: u32,
  data_bits: Option<u8>,
  stop_bits: Option<u8>,
  parity: Option<String>,
  flow_control: Option<String>,
}

pub fn op_webserial_open(
  state: &mut OpState,
  args: Value,
  _zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
  let args: OpenArgs = serde_json::from_value(args)?;

  let port = serialport::new(args.device, args.baud_rate)
    .data_bits(match args.data_bits {
      Some(7) => serialport::DataBits::Seven,
      Some(8) => serialport::DataBits::Eight,
      Some(_) => return Err(type_error("")),
      None => serialport::DataBits::Eight,
    })
    .stop_bits(match args.stop_bits {
      Some(1) => serialport::StopBits::One,
      Some(2) => serialport::StopBits::Two,
      Some(_) => return Err(type_error("")),
      None => serialport::StopBits::One,
    })
    .parity(match args.parity.as_deref() {
      Some("none") => serialport::Parity::None,
      Some("odd") => serialport::Parity::Odd,
      Some("even") => serialport::Parity::Even,
      Some(_) => return Err(type_error("")),
      None => serialport::Parity::None,
    })
    .flow_control(match args.flow_control.as_deref() {
      Some("none") => serialport::FlowControl::None,
      Some("hardware") => serialport::FlowControl::Hardware,
      Some(_) => return Err(type_error("")),
      None => serialport::FlowControl::None,
    })
    .open()?;

  let rid = state.resource_table.add(WebSerialPortResource(AsyncRefCell::new(port)));

  Ok(json!(rid))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadArgs {
  rid: u32,
}

pub fn op_webserial_read(
  state: &mut OpState,
  args: Value,
  zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
  let args: ReadArgs = serde_json::from_value(args)?;

  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |v| &v.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  port.read_exact(zero_copy[0].as_mut())?;

  Ok(json!({}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WriteArgs {
  rid: u32,
}

pub fn op_webserial_write(
  state: &mut OpState,
  args: Value,
  zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
  let args: WriteArgs = serde_json::from_value(args)?;

  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |v| &v.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  port.write_all(&*zero_copy[0])?;

  Ok(json!({}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetSignalsArgs {
  rid: u32,
  data_terminal_ready: Option<bool>,
  request_to_send: Option<bool>,
  #[serde(rename = "break")]
  break_: Option<bool>,
}

pub fn op_webserial_set_signals(
  state: &mut OpState,
  args: Value,
  _zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
  let args: SetSignalsArgs = serde_json::from_value(args)?;

  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |v| &v.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  if let Some(data_terminal_ready) = args.data_terminal_ready {
    port.write_data_terminal_ready(data_terminal_ready)?;
  }

  if let Some(request_to_send) = args.request_to_send {
    port.write_request_to_send(request_to_send)?;
  }

  if let Some(break_) = args.break_ {
    if break_ {
      port.set_break()?;
    } else {
      port.clear_break()?;
    }
  }

  Ok(json!({}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetSignalsArgs {
  rid: u32,
}

pub fn op_webserial_get_signals(
  state: &mut OpState,
  args: Value,
  _zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
  let args: GetSignalsArgs = serde_json::from_value(args)?;

  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |v| &v.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  Ok(json!({
    "dataCarrierDetect": port.read_carrier_detect()?,
    "clearToSend": port.read_clear_to_send()?,
    "ringIndicator": port.read_ring_indicator()?,
    "dataSetReady": port.read_data_set_ready()?,
  }))
}

/// Load and execute the javascript code.
pub fn init(isolate: &mut JsRuntime) {
  isolate
    .execute(
      "deno:op_crates/webserial/01_webserial.js",
      include_str!("01_webserial.js"),
    )
    .unwrap();
}
