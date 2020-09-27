use super::{Table, TEST_DB};
use proc_macro_error::abort_call_site;
use quote::quote;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::fs;

const MIGRATIONS_FILENAME: &str = "migrations.toml";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MigrationTables {
 table: Option<Vec<MigrationsForTable>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct MigrationsForTable {
 migrations: Option<Vec<String>>,
 name: Option<String>,
 target_schema: Option<String>,
}

/// CREATE TABLE
pub(super) fn create(table: &Table) -> proc_macro2::TokenStream {
 let sql = makesql_create(&table);

 TEST_DB.lock().unwrap().execute(&sql, params![]).unwrap_or_else(|e| {
  abort_call_site!("Error validating auto-generated CREATE TABLE statement:\n{}\n{:#?}", sql, e)
 });

 let target_migrations = make_migrations(&table);
 eprintln!("{}", sql);
 eprintln!("{:#?}", target_migrations);

 let mut path = std::env::current_dir().unwrap();
 path.push(MIGRATIONS_FILENAME);
 let path_lossy = path.to_string_lossy();

 let source_tables = match path.exists() {
  true => {
   let toml_str = fs::read_to_string(&path)
    .unwrap_or_else(|e| abort_call_site!("Unable to read {}: {:?}", path_lossy, e));

   let toml_decoded: MigrationTables = toml::from_str(&toml_str)
    .unwrap_or_else(|e| abort_call_site!("Unable to decode toml in {}: {:?}", path_lossy, e));

   toml_decoded
    .table
    .unwrap_or_else(|| abort_call_site!("{} already exists with no [[table]] entries?", path_lossy))
  }
  false => vec![],
 };

 eprintln!("SOURCE TABLES\n{:#?}", source_tables);

 let this_table: MigrationsForTable = MigrationsForTable {
  migrations: Some(target_migrations.clone()),
  name: Some(table.name.clone()),
  target_schema: Some(sql.clone()),
 };

 let mut dest_tables = MigrationTables { table: Some(vec![this_table.clone()]) };

 // Scan through the source tables, pushing to dest tables. If we match this_table, update it instead of pushing.

 // dest_tables.unwrap().push(this_table.clone());

 for t in source_tables {
  if t.name == this_table.name {
   // dest_tables.push(this_table.clone());
   // need to update it
  } else {
   dest_tables.table.as_mut().unwrap().push(t.clone());

   // let table_vec: &mut Option<Vec<MigrationsForTable>> = &mut dest_tables.table;
   // let options_content = table_vec.as_deref_mut().unwrap();
   // options_content.push(t);
   // options_content;

   // dest_tables.push(t);
  }
  //  dest_tables.push(if t.name.clone().unwrap() == table.name {
  //   let source_migrations = match &t.migrations {
  //    Some(m) => m.clone(),
  //    None => vec![],
  //   };

  //   let source_iter = source_migrations.iter();
  //   let mut target_iter = target_migrations.iter();

  //   for source in source_iter {
  //    let target = target_iter
  //     .next()
  //     .context("Source migrations not a strict subset of target migrations.")?;

  //    if source != target {
  //     eprintln!("TABLE\n{:#?}", table.name);
  //     eprintln!("SOURCE MIGRATIONS\n{:#?}", source_migrations);
  //     eprintln!("TARGET MIGRATIONS\n{:#?}", target_migrations);
  //     eprintln!("SOURCE LINE: {:#?}", source);
  //     eprintln!("TARGET LINE: {:#?}", target);

  //     bail!("Source migrations not a strict subset of target migrations. (See output above)");
  //    }
  //   }

  //   eprintln!("REMAINING: {:#?}", target_iter.len());

  //   // stage 1: verify that source migrations is a strict subset of target migrations

  //   MigrationsForTable {
  //    migrations: Some(target_migrations.clone()),
  //    name: t.name.clone(),
  //    target_schema: Some(sql.clone()),
  //   }
  //  } else {
  //   t.clone()
  //  });
 }

 eprintln!("DEST TABLES\n{:#?}", dest_tables);

 // let source_table = source_tables
 //  .iter()
 //  .find(|m| m.name.clone().unwrap() == table.name);

 // // eprintln!("HELLO {:#?}", x);

 // eprintln!("TABLE\n{:#?}", table.name);

 // eprintln!("SOURCE MIGRATIONS\n{:#?}", source_migrations);

 // eprintln!("TARGET MIGRATIONS\n{:#?}", target_migrations);

 let mut toml_str = String::new();
 let mut serializer = toml::Serializer::pretty(&mut toml_str);
 serializer.pretty_array_indent(2);

 dest_tables
  .serialize(&mut serializer)
  .unwrap_or_else(|e| abort_call_site!("Unable to serialize migrations toml: {:?}", e));

 // let foo = toml::to_string_pretty(&decoded).unwrap();
 // eprintln!("{}", toml_str);

 let migrationsfile = format!(
  r"# This file is auto-generated by Turbosql.
# It is used to create and apply automatic schema migrations.
# It should be checked into source control.
# Modifying it by hand may be dangerous; see the docs.

{}",
  &toml_str
 );

 fs::write(&path, migrationsfile)
  .unwrap_or_else(|e| abort_call_site!("Unable to write {}: {:?}", path_lossy, e));

 // eprintln!("TESTING SQL FOR {}", table.name);

 let table_name = table.name.clone();

 quote! {
  fn __turbosql_ensure_table_created() {
   static ONCE: ::turbosql::Lazy<()> = ::turbosql::Lazy::new(|| {
    ::turbosql::__ensure_table_created(#table_name, #sql, vec![#(#target_migrations),*]);
   });

   ::turbosql::Lazy::force(&ONCE);
  }
 }
}

fn makesql_create(table: &Table) -> String {
 let mut sql = format!("CREATE TABLE {} (\n", table.name);

 sql += table
  .columns
  .iter()
  .map(|c| format!(" {} {}", c.name, c.sqltype))
  .collect::<Vec<_>>()
  .join(",\n")
  .as_str();

 sql += "\n)";

 sql
}

// TODO make sure this works if user puts rowid member someplace other than first
// (or enforce first position)

fn make_migrations(table: &Table) -> Vec<String> {
 let mut vec = vec![format!("CREATE TABLE {} (rowid INTEGER PRIMARY KEY)", table.name)];

 let mut alters = table
  .columns
  .iter()
  .filter_map(|c| match (c.name.as_str(), c.sqltype) {
   ("rowid", "INTEGER PRIMARY KEY") => None,
   _ => Some(format!("ALTER TABLE {} ADD COLUMN {} {}", table.name, c.name, c.sqltype)),
  })
  .collect::<Vec<_>>();

 vec.append(&mut alters);

 vec
}