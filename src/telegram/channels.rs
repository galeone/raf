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

use log::{error, info};
use rusqlite::params;
use telexide_fork::{
    api::types::{CreateChatInviteLink, GetChat, GetChatAdministrators, SendMessage},
    model::{AdministratorMemberStatus, Chat, ChatMember},
    prelude::*,
};

use crate::persistence::types::{Channel, DBKey};

/// Returns all the channels owned by `user_id`.
///
/// # Arguments:
/// * `ctx` - Telexide `Context`
/// * `user_id` - The user ID
///
/// # Panics
/// Panics if the connection to the DB fails, or if the returned data is corrupt.
#[must_use]
pub fn get_all(ctx: &Context, user_id: i64) -> Vec<Channel> {
    let guard = ctx.data.read();
    let map = guard.get::<DBKey>().expect("db");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, link, name FROM channels WHERE registered_by = ? ORDER BY id ASC")
        .unwrap();

    let channels = stmt
        .query_map(params![user_id], |row| {
            Ok(Channel {
                id: row.get(0)?,
                registered_by: user_id,
                link: row.get(1)?,
                name: row.get(2)?,
            })
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    channels
}

/// Returns all the admins of the `chat_id`. In case of errors sends a message to the `user_id`
/// and logs with `error!`.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `chat_id` - The unique id of the group/chan under examination
/// * `user_id` - The user that requested this admin list.
///
/// # Panics
/// Panics if the Telegram server returns error.
pub async fn admins(ctx: &Context, chat_id: i64, user_id: i64) -> Vec<AdministratorMemberStatus> {
    let admins = ctx
        .api
        .get_chat_administrators(GetChatAdministrators { chat_id })
        .await;
    if admins.is_err() {
        let res = ctx
            .api
            .send_message(SendMessage::new(
                user_id,
                "Error! You must add this bot as admin of the group/channel.",
            ))
            .await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[register] send message {}", err);
        }
        return vec![];
    }
    let admins = admins.unwrap();

    admins
        .iter()
        .filter_map(|u| {
            if let ChatMember::Administrator(admin) = u {
                Some(admin.clone())
            } else {
                None
            }
        })
        .collect()
}

/// Tries to register a chat identified by its `chat_id`. The chat can be
/// - a channel
/// - a group
/// - a supergroup
///
/// Returns true in case of registration success.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `chat_id` - Unique identifier of the chat
/// * `registered_by` - Unique identifier of the `User` (`user.id`) that wants to register the
/// chat.
///
/// # Panics
///
/// Panics if the commincation with telegram fails, or if the database is failing.
pub async fn try_register(ctx: &Context, chat_id: i64, registered_by: i64) -> bool {
    // NOTE: we need this get_chat call because chat.invite_link is returned only by
    // calling GetChat: https://core.telegram.org/bots/api#chat
    info!("try_register begin");
    let (mut invite_link, username, title, is_channel) = {
        match ctx.api.get_chat(GetChat { chat_id }).await.unwrap() {
            Chat::Channel(c) => (c.invite_link, c.username, Some(c.title), true),
            Chat::Group(c) => (c.invite_link, c.username, Some(c.title), false),
            Chat::SuperGroup(c) => (c.invite_link, c.username, Some(c.title), false),
            Chat::Private(_) => (None, None, None, false),
        }
    };

    if invite_link.is_none() && username.is_none() {
        // Try to generate, it might happen (?) anyway is safe hence who cares
        if let Ok(invite) = ctx
            .api
            .create_chat_invite_link(CreateChatInviteLink {
                chat_id,
                expire_date: None,
                member_limit: None,
            })
            .await
        {
            invite_link = Some(invite.invite_link);
        }
    }

    let link: String = {
        if let Some(invite_link) = invite_link {
            invite_link.to_string()
        } else if let Some(username) = username {
            format!("https://t.me/{username}")
        } else {
            String::new()
        }
    };

    if link.is_empty() {
        // INFO: info is correct since if we are not able to extract these information
        // perhaps the received message is not from a chan/group/supergroup and
        // there's no need to send anything back to the user
        info!(
            "[register] Unable to extract invite link / username for {}",
            chat_id
        );
        return false;
    }

    let admins = admins(ctx, chat_id, registered_by).await;
    let mut found = false;
    let me = ctx.api.get_me().await.unwrap(); // the bot!
    for admin in admins {
        // permissions in channel and groups are a bit different
        if admin.user.is_bot
            && admin.user.id == me.id
            && admin.can_manage_chat
            && ((is_channel
                && admin.can_post_messages.is_some()
                && admin.can_post_messages.unwrap())
                || (!is_channel
                    && admin.can_pin_messages.is_some()
                    && admin.can_pin_messages.unwrap()))
        {
            found = true;
            break;
        }
    }

    if !found {
        let res = ctx
            .api
            .send_message(SendMessage::new(
                registered_by,
                "The bot must be admin of the channel/group, and shall be able to:\n\n\
                1. manage the chat.\n2. post messages\n3. pin messages",
            ))
            .await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[register] send message {}", err);
        }
        return false;
    }

    let title = title.unwrap();
    let res = {
        let guard = ctx.data.read();
        let map = guard.get::<DBKey>().expect("db");
        let conn = map.get().unwrap();

        conn.execute(
            "INSERT OR IGNORE INTO channels(id, registered_by, link, name) VALUES(?, ?, ?, ?)",
            params![chat_id, registered_by, link, title],
        )
    };

    if res.is_err() {
        let err = res.err().unwrap();
        error!("[register] {}", err);

        let res = ctx
            .api
            .send_message(SendMessage::new(registered_by, &err.to_string()))
            .await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[register] send message {}", err);
        }
        return false;
    }

    // from here below, the registration is succeded, hence if we fail in deliver a
    // message we dont' return false, because in the DB is all OK
    let res = ctx
        .api
        .send_message(SendMessage::new(
            registered_by,
            &format!("Channel/Group {title} registered succesfully!"),
        ))
        .await;

    if res.is_err() {
        let err = res.err().unwrap();
        error!("[final register] {}", err);
    }
    info!("try_register end");
    true
}
