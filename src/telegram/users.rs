use crate::persistence::types::{DBKey, User};
use rusqlite::params;
use telexide::prelude::*;

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

#[must_use]
pub fn owners(ctx: &Context) -> Vec<User> {
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT users.id, users.first_name, users.last_name, users.username \
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
