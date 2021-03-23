// Copyright 2018-2021 the Deno authors. All rights reserved. MIT license.

use deno_core::error::bad_resource_id;
use deno_core::error::custom_error;
use deno_core::error::resource_unavailable;
use deno_core::error::type_error;
use deno_core::error::AnyError;
use deno_core::serde_json::json;
use deno_core::serde_json::Value;
use deno_core::AsyncRefCell;
use deno_core::BufVec;
use deno_core::JsRuntime;
use deno_core::OpState;
use deno_core::RcRef;
use deno_core::Resource;
use deno_core::ZeroCopyBuf;
use serde::Deserialize;
use std::borrow::Cow;
use std::cell::RefCell;
use std::io::Read;
use std::io::Write;
use std::rc::Rc;

struct WebSerialPortResource(AsyncRefCell<Box<dyn serialport::SerialPort>>);

impl Resource for WebSerialPortResource {
  fn name(&self) -> Cow<str> {
    "webSerialPort".into()
  }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenArgs {
  device: String,
  baud_rate: u32,
  data_bits: Option<u8>,
  stop_bits: Option<u8>,
  parity: Option<String>,
  flow_control: Option<String>,
}

pub fn op_webserial_open(
  state: &mut OpState,
  args: OpenArgs,
  _zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
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

  let rid = state
    .resource_table
    .add(WebSerialPortResource(AsyncRefCell::new(port)));

  Ok(json!(rid))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadArgs {
  rid: u32,
}

pub async fn op_webserial_read(
  state: Rc<RefCell<OpState>>,
  args: ReadArgs,
  mut bufs: BufVec,
) -> Result<Value, AnyError> {
  let state = state.borrow_mut();
  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |r| &r.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  port.read_exact(bufs[0].as_mut())?;

  Ok(json!({}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteArgs {
  rid: u32,
}

pub async fn op_webserial_write(
  state: Rc<RefCell<OpState>>,
  args: WriteArgs,
  bufs: BufVec,
) -> Result<Value, AnyError> {
  let state = state.borrow_mut();
  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |r| &r.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  port.write_all(&*bufs[0])?;

  Ok(json!({}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSignalsArgs {
  rid: u32,
  data_terminal_ready: Option<bool>,
  request_to_send: Option<bool>,
  #[serde(rename = "break")]
  break_: Option<bool>,
}

pub fn op_webserial_set_signals(
  state: &mut OpState,
  args: SetSignalsArgs,
  _zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |r| &r.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  if let Some(data_terminal_ready) = args.data_terminal_ready {
    port
      .write_data_terminal_ready(data_terminal_ready)
      .map_err(err)?;
  }

  if let Some(request_to_send) = args.request_to_send {
    port.write_request_to_send(request_to_send).map_err(err)?;
  }

  if let Some(break_) = args.break_ {
    if break_ {
      port.set_break().map_err(err)?;
    } else {
      port.clear_break().map_err(err)?;
    }
  }

  Ok(json!({}))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSignalsArgs {
  rid: u32,
}

pub fn op_webserial_get_signals(
  state: &mut OpState,
  args: GetSignalsArgs,
  _zero_copy: &mut [ZeroCopyBuf],
) -> Result<Value, AnyError> {
  let resource = state
    .resource_table
    .get::<WebSerialPortResource>(args.rid)
    .ok_or_else(bad_resource_id)?;
  let mut port = RcRef::map(&resource, |r| &r.0)
    .try_borrow_mut()
    .ok_or_else(resource_unavailable)?;

  Ok(json!({
    "dataCarrierDetect": port.read_carrier_detect().map_err(err)?,
    "clearToSend": port.read_clear_to_send().map_err(err)?,
    "ringIndicator": port.read_ring_indicator().map_err(err)?,
    "dataSetReady": port.read_data_set_ready().map_err(err)?,
  }))
}

fn err(e: serialport::Error) -> AnyError {
  custom_error("DOMException", e.description)
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
