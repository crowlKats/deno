use deno_core::error::AnyError;
use deno_core::{OpState, Resource, ResourceId};
use rusqlite::params;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde::Serialize;
use std::borrow::Cow;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

struct IndexedDBResource(Connection);

impl Resource for IndexedDBResource {
  fn name(&self) -> Cow<str> {
    "indexedDB".into()
  }
}

pub async fn op_indexeddb_open_database(
  state: Rc<RefCell<OpState>>,
  name: String,
  version: Option<u64>,
) -> Result<(ResourceId, bool), AnyError> {
  let path = state
    .borrow()
    .try_borrow::<super::OriginStorageDir>()
    .ok_or_else(|| {
      super::DomExceptionNotSupportedError::new(
        "IndexedDB is not supported in this context.",
      )
    })?;

  let path = path.0.join("indexeddb");
  std::fs::create_dir_all(path)?;
  let full_path = path.join(name);
  let exists = full_path.exists();
  let mut upgraded = false;
  let conn = tokio::task::spawn_blocking(move || {
    let conn = Connection::open(full_path)?;

    if exists {
      let mut stmt = conn.prepare("SELECT version FROM metadata")?;
      let curr_version: u64 =
        stmt.query_row(params![], |row| row.get(0)).unwrap();

      if let Some(version) = version {
        match curr_version.cmp(&version) {
          Ordering::Less => {
            conn.execute(
              "REPLACE INTO metadata (version) VALUES (?)",
              params![version],
            )?;
            upgraded = true;
            // TODO: upgrade
          }
          Ordering::Equal => {}
          Ordering::Greater => {
            return Err(super::DomExceptionVersionError::new("").into()); // TODO
          }
        }
      }
    } else {
      conn.execute(
        "CREATE TABLE metadata (version UNSIGNED BIG INT DEFAULT 0 NOT NULL)",
        params![],
      )?;
      conn.execute("INSERT metadata (version) VALUES (?)", params![])?;
    }

    Ok(conn)
  })
  .await??;

  let rid = state
    .borrow_mut()
    .resource_table
    .add(IndexedDBResource(conn));

  Ok((rid, upgraded))
}

#[derive(Serialize)]
pub struct DatabaseInfo {
  name: String,
  version: u64,
}

pub async fn op_indexeddb_databases(
  state: Rc<RefCell<OpState>>,
  _: (),
  _: (),
) -> Result<Vec<DatabaseInfo>, AnyError> {
  let path = state
    .borrow()
    .try_borrow::<super::OriginStorageDir>()
    .ok_or_else(|| {
      super::DomExceptionNotSupportedError::new(
        "IndexedDB is not supported in this context.",
      )
    })?;

  let databases = tokio::task::spawn_blocking(move || {
    std::fs::read_dir(path.0.join("indexeddb"))?
      .map(|entry| {
        let entry = entry.unwrap();
        let conn = Connection::open(entry.path()).unwrap();
        let mut stmt = conn.prepare("SELECT version FROM metadata")?;
        let version = stmt.query_row(params![], |row| row.get(0)).unwrap();
        DatabaseInfo {
          name: entry.file_name().into_string().unwrap(),
          version,
        }
      })
      .collect()
  })
  .await?;

  Ok(databases)
}
