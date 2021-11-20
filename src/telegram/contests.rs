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

use chrono::{DateTime, Utc};
use log::error;
use rusqlite::params;
use telexide::{api::types::GetChatMember, prelude::*};

use crate::persistence::types::{Contest, DBKey, Rank};
use crate::telegram::users;

use std::string::ToString;

/// Returns the `Contest` with the specified `id`, if exists.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `id` - The ID (`RaF` generated) of the contest to search.
///
/// # Panics
/// Panics if the connection to the DB fails, or if the returned data is corrupt.
#[must_use]
pub fn get(ctx: &Context, id: i64) -> Option<Contest> {
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare("SELECT name, prize, end, started_at, chan, stopped FROM contests WHERE id = ?")
        .unwrap();
    let mut iter = stmt
        .query_map(params![id], |row| {
            Ok(Contest {
                id,
                name: row.get(0)?,
                prize: row.get(1)?,
                end: row.get(2)?,
                started_at: row.get(3)?,
                chan: row.get(4)?,
                stopped: row.get(5)?,
            })
        })
        .unwrap();
    let c = iter.next().unwrap();
    if let Ok(c) = c {
        return Some(c);
    }
    None
}

/// Returns all the `Contest` created for the channel with ID `id`.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `chan` - The ID (Telegram generated) of the Channel.
///
/// # Panics
/// Panics if the connection to the DB fails, or if the returned data is corrupt.
#[must_use]
pub fn get_all(ctx: &Context, chan: i64) -> Vec<Contest> {
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, prize, end, started_at, stopped FROM contests WHERE chan = ? ORDER BY end DESC",
        )
        .unwrap();

    let contests = stmt
        .query_map(params![chan], |row| {
            Ok(Contest {
                id: row.get(0)?,
                name: row.get(1)?,
                prize: row.get(2)?,
                end: row.get(3)?,
                started_at: row.get(4)?,
                stopped: row.get(5)?,
                chan,
            })
        })
        .unwrap()
        .map(std::result::Result::unwrap)
        .collect();
    contests
}

/// Returns rank for the `contest`, already oredered by number of invites accepted in descending
/// order.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `contest` - The `Contest` under examination
///
/// # Panics
/// Panics if the connection to the DB fails, or if the returned data is corrupt.
#[must_use]
pub fn ranking(ctx: &Context, contest: &Contest) -> Vec<Rank> {
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    // NOTE: the ordering ALSO via t.source is required to give a meaningful order (depending on
    // the id, hence jsut to have them different) in case of equal rank
    let mut stmt = conn
            .prepare(
                "SELECT ROW_NUMBER() OVER (ORDER BY t.c, t.source DESC) AS r, t.c, t.source
                FROM (SELECT COUNT(*) AS c, source FROM invitations WHERE contest = ? GROUP BY source) AS t",
            )
            .unwrap();
    stmt.query_map(params![contest.id], |row| {
        Ok(Rank {
            rank: row.get(0)?,
            invites: row.get(1)?,
            user: users::get(ctx, row.get(2)?).unwrap(),
        })
    })
    .unwrap()
    .map(std::result::Result::unwrap)
    .collect::<Vec<Rank>>()
}

/// Possible errors while creating a Contest
#[derive(Debug, Clone)]
pub enum Error {
    /// Error while parsing the user inserted date
    ParseError(chrono::format::ParseError),
    /// Generic error we want to report to the user as a string
    GenericError(String),
}

impl From<chrono::format::ParseError> for Error {
    /// Returns `Error::ParseError`
    fn from(error: chrono::format::ParseError) -> Error {
        Error::ParseError(error)
    }
}

impl From<String> for Error {
    /// Returns `Error::GenericError`
    fn from(error: String) -> Error {
        Error::GenericError(error)
    }
}

impl std::fmt::Display for Error {
    /// Format all the possible errors
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::ParseError(error) => write!(f, "DateTime parse {}", error),
            Error::GenericError(error) => write!(f, "{}", error),
        }
    }
}

/// Parse the input `text` and creates a valid `Contest` associated to the chan.
///
/// # Arguments
///
/// * `text` - A string slice holding the user inserted text
/// * `chan` - The channel to associate with the Contest in case of success
///
/// # Errors
/// If the parsing from text fails for whatever reason, it returns an `Error`
/// that contains a detail. In case of failed parsing, it's a `Error::ParseError(e)`
/// otherwise is a `Error::GenericError(s)` with a string containing the reason
/// of the failure.
pub fn from_text(text: &str, chan: i64) -> Result<Contest, Error> {
    let rows = text
        .split('\n')
        .skip_while(|r| r.is_empty())
        .collect::<Vec<&str>>();
    if rows.len() != 3 {
        return Err(format!("failed because row.len() != 3. Got: {}", rows.len()).into());
    }
    let id = -1;
    let name = rows[0].to_string();
    let prize = rows[2].to_string();
    // user input: YYYY-MM-DD hh:mm TZ, needs to become
    // YYYY-MM-DD hh:mm:ss TZ to get enough data to create a datetime object
    let add_seconds = |row: &str| -> String {
        let mut elements = row
            .split_whitespace()
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        if elements.len() != 3 {
            return row.to_string();
        }
        // 0: YYYY-MM-DD
        // 1: hh:mm
        // 2: TZ
        elements[1] += ":00";
        elements.join(" ")
    };
    let now = Utc::now();
    let end: DateTime<Utc> =
        DateTime::parse_from_str(&add_seconds(rows[1]), "%Y-%m-%d %H:%M:%S %#z")?.into();
    if end < now {
        return Err("End date can't be in the past".to_string().into());
    }
    Ok(Contest {
        id,
        end,
        name,
        prize,
        chan,
        stopped: false,
        started_at: None,
    })
}

/// Count the users that partecipated to the `contest`
///
/// # Arguments
///
/// * `ctx`: The telexide ctx, used to get the db
/// * `contest`: The Contest under examination
///
/// # Panics
/// Panics if the connection to the DB fails, or if the returned data is corrupt.
#[must_use]
pub fn count_users(ctx: &Context, contest: &Contest) -> i64 {
    struct Counter {
        value: i64,
    }
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare("SELECT COUNT(id) FROM invitations WHERE contest = ?")
        .unwrap();
    let vals = stmt
        .query_map(params![contest.id], |row| {
            Ok(Counter { value: row.get(0)? })
        })
        .unwrap()
        .map(|count| count.unwrap_or(Counter { value: -1 }).value)
        .collect::<Vec<i64>>();
    if vals.is_empty() {
        return 0;
    }
    vals[0]
}

/// Function to call to verify that the joined users are still in the channel.
/// NOTE: this function is async because it uses the async `ctx.api.get_chat_member`
/// function to check if the user is still inside the channel referenced by the `contest`.
///
/// # Arguments
/// * `ctx`: The telexide ctx, used to get the db
/// * `contest`: The Contest under examination
///
/// # Panics
/// Panics if the connection to the DB fails, or if the returned data is corrupt.
pub async fn validate_users(ctx: &Context, contest: &Contest) {
    struct InnerUser {
        id: i64,
    }
    let users = {
        let guard = ctx.data.read();
        let map = guard.get::<DBKey>().expect("db");
        let conn = map.get().unwrap();
        let mut stmt = conn
            .prepare("SELECT dest FROM invitations WHERE contest = ?")
            .unwrap();
        stmt.query_map(params![contest.id], |row| Ok(InnerUser { id: row.get(0)? }))
            .unwrap()
            .map(|user| user.unwrap().id)
            .collect::<Vec<i64>>()
    };

    for user in users {
        let member = ctx
            .api
            .get_chat_member(GetChatMember {
                chat_id: contest.chan,
                user_id: user,
            })
            .await;

        let in_channel = member.is_ok();
        if !in_channel {
            let res = {
                let guard = ctx.data.read();
                let map = guard.get::<DBKey>().expect("db");
                let conn = map.get().unwrap();
                let mut stmt = conn
                    .prepare("DELETE FROM invitations WHERE dest = ? and contest = ?")
                    .unwrap();
                stmt.execute(params![user, contest.id])
            };
            if res.is_err() {
                error!("[users validation] {}", res.err().unwrap());
            }
        }
    }
}
