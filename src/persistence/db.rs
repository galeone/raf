// Copyright 2021 Paolo Galeone <nessuno@nerdz.eu>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//    http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use r2d2_sqlite::SqliteConnectionManager;

/// Database schema definition. Transaction executed every time a new connection
/// pool is requested (usually, once at the application startup).
///
/// `being_managed_channels`, as the name suggests, is the channel that the owner (
/// hence `channels.registered_by` == owner) is managing.
///
/// NOTE: `being_contacted_users` and `being_managed_channels` are tables required because
/// there are moments in the flow, where the user should send "complex" messages, but these
/// "complex" messages are outside the FSM created by the `callback_handler`
/// (FSM created naturally because all the callbacks invokes the same method).
const SCHEMA: &str = "BEGIN;
CREATE TABLE IF NOT EXISTS users (
   id   INTEGER PRIMARY KEY NOT NULL,
   first_name TEXT NOT NULL,
   last_name TEXT,
   username TEXT
);
CREATE TABLE IF NOT EXISTS channels (
   id   INTEGER PRIMARY KEY NOT NULL,
   registered_by INTEGER NOT NULL,
   link TEXT NOT NULL,
   name TEXT NOT NULL,
   FOREIGN KEY(registered_by) REFERENCES users(id),
   UNIQUE(id, registered_by)
);
CREATE TABLE IF NOT EXISTS invitations(
   id   INTEGER PRIMARY KEY AUTOINCREMENT,
   date TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
   source INTEGER NOT NULL,
   dest INTEGER NOT NULL,
   chan INTEGER NOT NULL,
   contest INTEGER NOT NULL,
   FOREIGN KEY(source) REFERENCES users(id),
   FOREIGN KEY(dest) REFERENCES users(id),
   FOREIGN KEY(chan) REFERENCES channels(id),
   FOREIGN KEY(contest) REFERENCES contests(id),
   CHECK (source <> dest),
   UNIQUE(source, dest, chan)
);
CREATE TABLE IF NOT EXISTS contests(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  prize TEXT NOT NULL,
  end TIMESTAMP NOT NULL,
  chan INTEGER NOT NULL,
  started_at TIMESTAMP NULL,
  stopped BOOL NOT NULL DEFAULT FALSE,
  FOREIGN KEY(chan) REFERENCES channels(id),
  UNIQUE(name, chan)
);
CREATE TABLE IF NOT EXISTS being_managed_channels(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  chan INTEGER NOT NULL,
  FOREIGN KEY(chan) REFERENCES channels(id)
);
CREATE TABLE IF NOT EXISTS being_contacted_users(
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  user INTEGER NOT NULL,
  owner INTEGER NOT NULL,
  contest INTEGER NOT NULL,
  contacted BOOL NOT NULL DEFAULT FALSE,
  FOREIGN KEY(user) REFERENCES users(id),
  FOREIGN KEY(owner) REFERENCES users(id)
);
COMMIT;";

/// Creates a connection pool to the `SQLite` database, whose name is always
/// "raf.db" and it's always in the current working directory of the application.
///
/// Foreign keys are enabled in the `SQLite` instance.
///
/// # Panics
/// Panics if the connection with the db fails.
#[must_use]
pub fn connection() -> r2d2::Pool<SqliteConnectionManager> {
    let manager = SqliteConnectionManager::file("raf.db")
        .with_init(|c| c.execute_batch("PRAGMA foreign_keys=1;"));
    let pool = r2d2::Pool::builder().max_size(15).build(manager).unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute_batch(SCHEMA).unwrap();
    }

    pool
}
