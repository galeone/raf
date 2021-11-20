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

use rusqlite::params;
use telexide::prelude::*;

use crate::persistence::types::{DBKey, User};

/// Returns the `User` with the specified `id`, if any.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `id` - The user ud
///
/// # Panics
/// Panics if the connection to the db fails.
#[must_use]
pub fn get(ctx: &Context, id: i64) -> Option<User> {
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare("SELECT first_name, last_name, username FROM users WHERE id = ?")
        .unwrap();
    let mut iter = stmt
        .query_map(params![id], |row| {
            Ok(User {
                id,
                first_name: row.get(0)?,
                last_name: row.get(1)?,
                username: row.get(2)?,
            })
        })
        .unwrap();
    if let Some(user) = iter.next() {
        return match user {
            Ok(user) => Some(user),
            Err(_) => None,
        };
    }
    None
}

/// Returns the complete list of owners. Owners are the users who registered a channel/group.
///
/// # Arguments
/// * `ctx` - Telexide context
///
/// # Panics
/// Panics if the connection to the db fails.
#[must_use]
pub fn owners(ctx: &Context) -> Vec<User> {
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT users.id, users.first_name, users.last_name, users.username \
        FROM users INNER JOIN channels ON users.id = channels.registered_by \
        ORDER BY users.id",
        )
        .unwrap();

    let users = stmt
        .query_map(params![], |row| {
            Ok(User {
                id: row.get(0)?,
                first_name: row.get(1)?,
                last_name: row.get(2)?,
                username: row.get(3)?,
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    users
}
