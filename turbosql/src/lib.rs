/*! # Turbosql: Easy Data Persistence Layer, backed by SQLite

WORK IN PROGRESS, use at your own risk. :)

Macros for easily persisting Rust `struct`s to an on-disk SQLite database and later retrieving them, optionally based on your own predicates.

```rust
use turbosql::Turbosql;

[derive(Turbosql)]
struct Person {
 rowid: Option<i64>,  // rowid member required & enforced at compile time
 name: String,
 age: Option<i64>,
 image_jpg: Option<Blob>
}
```

## Design Goals

- API with minimal cognitive complexity and boilerplate
- High performance
- Reliable storage
- Surface the power of SQL — make simple things easy, and complex things possible
- In the spirit of Rust, move as many errors as possible to compile time

### License: MIT OR Apache-2.0
*/

#![allow(unused_imports)]

use log::{debug, error, info, trace, warn};
use rusqlite::{Connection, OpenFlags, Statement};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use std::sync::Mutex;

// re-export

#[doc(hidden)]
pub use once_cell::sync::Lazy;
#[doc(hidden)]
pub use rusqlite::{
 params, types::FromSql, types::FromSqlResult, types::ToSql, types::ToSqlOutput, types::Value,
 types::ValueRef, Error, Result,
};
#[doc(hidden)]
pub use serde::Serialize;
pub use turbosql_macros::{
 execute, execute_unchecked, select, select_cte, select_cte_unchecked, select_unchecked, Turbosql,
};

/// Wrapper for `Vec<u8>` that provides `Read`, `Write` and `Seek` traits.
pub type Blob = Vec<u8>;

// #[derive(Debug)]
// pub struct Blob {
//  table: String,
//  column: String,
//  rowid: i64,
//  len: i64,
//  bytes: Option<Vec<u8>>,
// }

struct DbPath {
 path: PathBuf,
 opened: bool,
}

static __DB_PATH: Lazy<Mutex<DbPath>> = Lazy::new(|| {
 let cur_exe = match std::env::current_exe() {
  Ok(path) => match path.file_stem() {
   Some(stem) => Some(stem.to_str().unwrap().to_string()), // TODO: remove unwrap
   None => None,
  },
  Err(_) => None,
 };

 Mutex::new(DbPath {
  path: Path::new(&match cur_exe {
   Some(name) => format!("{}.sqlite", name),
   None => "turbosql.sqlite".to_owned(),
  })
  .to_owned(),
  opened: false,
 })
});

// static __DB_OPENED: AtomicBool = AtomicBool::new(false);

#[doc(hidden)]
pub static __TURBOSQL_DB: Lazy<Mutex<Connection>> = Lazy::new(|| {
 debug!("in make_connection");

 let mut db_path = __DB_PATH.lock().unwrap();

 db_path.opened = true;

 // We are handling the mutex, so SQLite can be opened in no-mutex mode; see:
 // http://sqlite.1065341.n5.nabble.com/SQLITE-OPEN-FULLMUTEX-vs-SQLITE-OPEN-NOMUTEX-td104785.html

 let conn = Connection::open_with_flags(
  &db_path.path,
  OpenFlags::SQLITE_OPEN_READ_WRITE
   | OpenFlags::SQLITE_OPEN_CREATE
   | OpenFlags::SQLITE_OPEN_NO_MUTEX,
 )
 .unwrap();

 // something something autotrim file mode on row deletion

 conn
  .execute_batch(
   r"
    PRAGMA journal_mode=WAL;
    PRAGMA wal_autocheckpoint=8000;
    PRAGMA synchronous=NORMAL;
   ",
  )
  .unwrap();

 Mutex::new(conn)
});

/// Set the local path and filename where Turbosql will store the underlying SQLite database.
///
/// Must be called before any usage of Turbosql macros or will return an error.
/// (Should actually be a std::path::Path?)
pub fn set_db_path(path: &Path) -> Result<(), anyhow::Error> {
 let mut db_path = __DB_PATH.lock().unwrap();

 if db_path.opened {
  return Err(anyhow::anyhow!("Trying to set path when DB is already opened"));
 }

 db_path.path = path.to_owned();

 Ok(())
}

#[doc(hidden)]
/// TODO: Remove in favor of execute! macro
pub fn execute<P>(sql: &str, params: P) -> rusqlite::Result<usize>
where
 P: IntoIterator,
 P::Item: ToSql,
{
 let db = __TURBOSQL_DB.lock().unwrap();
 db.execute(sql, params)
}

#[doc(hidden)]
pub fn __ensure_table_created(
 table_name: &'static str,
 create_sql: &'static str,
 migrations: Vec<&'static str>,
) {
 let db = __TURBOSQL_DB.lock().unwrap();

 let result =
  db.query_row("SELECT sql FROM sqlite_master WHERE name = ?", params![table_name], |row| {
   let sql: String = row.get(0).unwrap();
   Ok(sql)
  });

 match result {
  Err(rusqlite::Error::QueryReturnedNoRows) => {
   // no table yet, create
   db.execute_batch(create_sql).unwrap();
  }
  Err(err) => {
   panic!(err);
  }
  Ok(sql) => {
   // already have table, verify it's the same schema
   if sql != create_sql {
    println!("{}", sql);
    println!("{}", create_sql);
    panic!("Turbosql sqlite schema does not match! Delete database file to continue.");
   }
  }
 }
}

// pub fn add_table(sql: &str) {
//  debug!("in add_table");
// }
