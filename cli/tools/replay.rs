// Copyright 2018-2026 the Deno authors. MIT license.

use std::path::Path;
use std::sync::Arc;

use deno_core::error::AnyError;
use deno_runtime::WorkerExecutionMode;

use crate::args::Flags;
use crate::args::ReplayFlags;
use crate::factory::CliFactory;

pub async fn replay(
  flags: Arc<Flags>,
  replay_flags: ReplayFlags,
) -> Result<i32, AnyError> {
  let trace_path = Path::new(&replay_flags.trace_file);

  if !trace_path.exists() {
    log::error!("Trace file not found: {}", replay_flags.trace_file);
    return Ok(1);
  }

  if replay_flags.info {
    return print_trace_info(trace_path);
  }

  if replay_flags.validate {
    return validate_trace(trace_path);
  }

  run_replay(flags).await
}

async fn run_replay(flags: Arc<Flags>) -> Result<i32, AnyError> {
  let factory = CliFactory::from_flags(flags);
  let cli_options = factory.cli_options()?;
  let main_module = cli_options.resolve_main_module()?.clone();
  let preload_modules = cli_options.preload_modules()?;
  let require_modules = cli_options.require_modules()?;

  let worker_factory = factory.create_cli_main_worker_factory().await?;
  let mut worker = worker_factory
    .create_main_worker(
      WorkerExecutionMode::Run,
      main_module,
      preload_modules,
      require_modules,
    )
    .await?;

  let exit_code = worker.run().await?;
  Ok(exit_code)
}

fn print_trace_info(path: &Path) -> Result<i32, AnyError> {
  let info = deno_core::trace::read_trace_info(path)?;

  let recorded_at = format_timestamp(info.header.recorded_at);

  println!("Trace file: {}", path.display());
  println!("Format version: {}", info.header.version);
  println!("Deno version: {}", info.header.deno_version);
  println!("V8 version: {}", info.header.v8_version);
  println!("Target: {}", info.header.target);
  println!("Recorded at: {recorded_at}");
  println!("Entry point: {}", info.header.entry_point);
  println!("Registered ops: {}", info.header.op_names.len());
  println!("Recorded events: {}", info.event_count);
  println!("File size: {}", human_readable_size(info.file_size));

  Ok(0)
}

fn validate_trace(path: &Path) -> Result<i32, AnyError> {
  let info = deno_core::trace::read_trace_info(path)?;

  println!("Trace file: {}", path.display());
  println!("Format version: {}", info.header.version);
  println!("Events: {}", info.event_count);
  println!("Status: Valid");

  Ok(0)
}

fn format_timestamp(ms: u64) -> String {
  if ms == 0 {
    return "unknown".into();
  }
  let secs = ms / 1000;
  let millis = ms % 1000;
  format!("{secs}.{millis:03} (unix epoch)")
}

fn human_readable_size(bytes: u64) -> String {
  const KB: u64 = 1024;
  const MB: u64 = KB * 1024;
  const GB: u64 = MB * 1024;

  if bytes >= GB {
    format!("{:.2} GB", bytes as f64 / GB as f64)
  } else if bytes >= MB {
    format!("{:.2} MB", bytes as f64 / MB as f64)
  } else if bytes >= KB {
    format!("{:.2} KB", bytes as f64 / KB as f64)
  } else {
    format!("{bytes} B")
  }
}
