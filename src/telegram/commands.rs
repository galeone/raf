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

use data_encoding::BASE64URL;
use log::{error, info};
use rusqlite::params;
use std::collections::HashMap;

use telexide_fork::{
    api::types::SendMessage,
    framework::{CommandError, CommandResult},
    model::{InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyMarkup},
    prelude::*,
};

use crate::{
    persistence::types::{Channel, DBKey, NameKey, RankContest},
    telegram::{
        channels, contests,
        messages::{display_main_commands, escape_markdown},
        users,
    },
};

/// Rank command. Shows to the user his/her rank for every joined challenge.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `message` - Received message with the command inside
///
/// # Panics
/// Panics if the connection to the db fails, or if telegram servers return error.
#[command(description = "Your rank in the challenges you joined")]
pub async fn rank(ctx: Context, message: Message) -> CommandResult {
    info!("rank command begin");
    let sender_id = message.from.clone().unwrap().id;
    let rank_per_user_contest = {
        let guard = ctx.data.read();
        let map = guard.get::<DBKey>().expect("db");
        let conn = map.get().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT ROW_NUMBER() OVER (ORDER BY t.c, t.source DESC) AS r, t.contest
                FROM (SELECT COUNT(*) AS c, contest, source FROM invitations GROUP BY contest, source) AS t
                WHERE t.source = ?",
            )
            .unwrap();

        let mut iter = stmt
            .query_map(params![sender_id], |row| {
                Ok(RankContest {
                    rank: row.get(0)?,
                    c: contests::get(&ctx, row.get(1)?).unwrap(),
                })
            })
            .unwrap()
            .peekable();
        if iter.peek().is_some() && iter.peek().unwrap().is_ok() {
            iter.map(std::result::Result::unwrap)
                .collect::<Vec<RankContest>>()
        } else {
            vec![]
        }
    };

    let text = if rank_per_user_contest.is_empty() {
        "You haven't partecipated in any contest yet!".to_string()
    } else {
        let mut m = "Your rankings\n\n".to_string();
        for rank_contest in rank_per_user_contest {
            let c = rank_contest.c;
            let rank = rank_contest.rank;
            m += &format!("Contest \"{}({})\": ", c.name, c.end);
            if rank == 1 {
                m += "\u{1f947}#1!";
            } else if rank <= 3 {
                m += &format!("\u{1f3c6} #{rank}");
            } else {
                m += &format!("#{rank}");
            }
            m += "\n";
        }
        m
    };
    let mut reply = SendMessage::new(sender_id, &escape_markdown(&text, None));
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let res = ctx.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[rank] {}", err);
    }

    display_main_commands(&ctx, sender_id).await;
    info!("rank command end");
    Ok(())
}

/// Help command. Shows to the user the help menu with the complete command list.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `message` - Received message with the command inside
#[command(description = "Help menu")]
pub async fn help(ctx: Context, message: Message) -> CommandResult {
    info!("help command begin");
    let sender_id = message.from.clone().unwrap().id;
    let text = escape_markdown(
        "I can create contests based on the referral strategy. \
        The user that referes more (legit) users will win a prize!\n\n\
        You can control me by sending these commands:\n\n\
        /register - Register a channel/group to the bot\n\
        /list - List your registered groups/channels\n\
        /contest - Start/Manage the referral contest\n\
        /rank - Your rank in the challenges you joined\n\
        /help - This menu",
        None,
    );
    let mut reply = SendMessage::new(sender_id, &text);
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let res = ctx.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[help] {}", err);
    }
    info!("help command end");
    Ok(())
}

/// Contest command. Start/Manage the referral contest.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `message` - Received message with the commands inside
///
/// # Panics
/// Panics if the connection to the db fails, or if telegram servers return error.
#[command(description = "Start/Manage the referral contest")]
pub async fn contest(ctx: Context, message: Message) -> CommandResult {
    info!("contest command begin");
    let sender_id = message.from.clone().unwrap().id;
    let channels = channels::get_all(&ctx, sender_id);

    if channels.is_empty() {
        let reply = SendMessage::new(sender_id, "You have no registered groups/channels!");
        let res = ctx.api.send_message(reply).await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[list channels] {}", err);
        }
        display_main_commands(&ctx, sender_id).await;
    } else {
        let mut reply = SendMessage::new(sender_id, "Select the group/channel you want to manage");

        let mut partition_size: usize = channels.len() / 2;
        if partition_size < 2 {
            partition_size = 1;
        }
        let inline_keyboard: Vec<Vec<InlineKeyboardButton>> = channels
            .chunks(partition_size)
            .map(|chunk| {
                chunk
                    .iter()
                    .map(|channel| InlineKeyboardButton {
                        text: channel.name.clone(),
                        // manage, channel id
                        callback_data: Some(format!("manage {}", channel.id)),
                        callback_game: None,
                        login_url: None,
                        pay: None,
                        switch_inline_query: None,
                        switch_inline_query_current_chat: None,
                        url: None,
                    })
                    .collect()
            })
            .collect();
        reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
            inline_keyboard,
        }));
        reply.set_parse_mode(&ParseMode::MarkdownV2);
        let res = ctx.api.send_message(reply).await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[list channels] {}", err);
        }
    }

    info!("contest command end");
    Ok(())
}

/// Start command. Depending on the `message` content executes different actions.
/// In any case, it adds the users to the list of the known users.
///
/// - If the message contains only `/start` it starts the bot with an hello message.
/// - If the message contains the base64 encoded parameters: user, channel, contest
/// this is an invitation link from user, to join the channel, because of contest.
/// - If the message contains the base64 encoded parameters: channel, contest
/// this is the link `RaF` generated and posted to the channel, that ever partecipant uses to
/// generate its own referral link.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `message` - Received message with the commands inside
///
/// # Panics
/// Panics if the connection to the db fails, or if telegram servers return error.
#[command(description = "Start the Bot")]
pub async fn start(ctx: Context, message: Message) -> CommandResult {
    info!("start command begin");
    let sender_id = message.from.clone().unwrap().id;
    // We should also check that at that time the user is not inside the chan
    // and that it comes to the channel only by following this custom link
    // with all the process (referred -> what channel? -> click in @channel
    // (directly from the bot, hence save the chan name) -> joined
    // Once done, check if it's inside (and save the date).

    // On start, save the user ID if not already present
    let res = {
        let guard = ctx.data.read();
        let map = guard.get::<DBKey>().expect("db");
        let conn = map.get().unwrap();
        let user = message.from.clone().unwrap();

        conn.execute(
            "INSERT OR IGNORE INTO users(id, first_name, last_name, username) VALUES(?, ?, ?, ?)",
            params![user.id, user.first_name, user.last_name, user.username,],
        )
    };
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[insert user] {}", err);
        ctx.api
            .send_message(SendMessage::new(
                sender_id,
                &format!("[insert user] {err}"),
            ))
            .await?;
    }

    // ?start=base64encode(source=<uid>&chan=<chan id>)
    // message = "start base64encode(source=ecc)"
    // source AND chan == invitation
    // chan ALONE = sent by the bot inside the chan, we have to generate the referring link
    // (encode with source = current user and this chan) that he can use to share the invite
    let text = message.get_text().unwrap();
    let mut split = text.split_ascii_whitespace();
    split.next(); // /start
    if let Some(encoded_params) = split.next() {
        let params = BASE64URL.decode(encoded_params.as_bytes())?;
        let params: HashMap<_, _> = url::form_urlencoded::parse(params.as_slice()).collect();
        info!("start params decoded: {:?}", params);

        let source = if params.contains_key("source") {
            params["source"].parse::<i64>().unwrap_or(-1)
        } else {
            -1
        };
        let chan = if params.contains_key("chan") {
            params["chan"].parse::<i64>().unwrap_or(-1)
        } else {
            -1
        };
        let contest_id = if params.contains_key("contest") {
            params["contest"].parse::<i64>().unwrap_or(-1)
        } else {
            -1
        };

        let (user, channel, c) = {
            let guard = ctx.data.read();
            let map = guard.get::<DBKey>().expect("db");
            let conn = map.get().unwrap();
            let mut stmt = conn
                .prepare("SELECT link, name, registered_by FROM channels WHERE id = ?")
                .unwrap();

            let channel = stmt
                .query_map(params![chan], |row| {
                    Ok(Channel {
                        id: chan,
                        link: row.get(0)?,
                        name: row.get(1)?,
                        registered_by: row.get(2)?,
                    })
                })
                .unwrap()
                .map(std::result::Result::unwrap)
                .next();

            let user = users::get(&ctx, source);
            let c = contests::get(&ctx, contest_id);
            (user, channel, c)
        };

        // Error
        if user.is_none() && channel.is_none() {
            ctx.api
                .send_message(SendMessage::new(
                    sender_id,
                    "Something wrong with the group/channel or the user that's inviting you.\n\
                    Contact the support.",
                ))
                .await?;
            return Err(CommandError(
                "Something wrong with the group/channel or the user that's inviting you".to_owned(),
            ));
            // Invite
        } else if user.is_some() && channel.is_some() && c.is_some() {
            let user = user.unwrap();
            let channel = channel.unwrap();
            let c = c.unwrap();

            let mut reply = SendMessage::new(
                sender_id,
                &escape_markdown(
                    &format!(
                        "{}{}{} invited you to join {}",
                        user.first_name,
                        match user.last_name {
                            Some(last_name) => format!(" {last_name}"),
                            None => String::new(),
                        },
                        match user.username {
                            Some(username) => format!(" (@{username})"),
                            None => String::new(),
                        },
                        channel.name
                    ),
                    None,
                ),
            );

            let inline_keyboard = vec![vec![
                InlineKeyboardButton {
                    text: "Accept \u{2705}".to_owned(),
                    // tick, source, dest, chan
                    callback_data: Some(format!(
                        "\u{2705} {} {} {} {}",
                        user.id,
                        message.from.clone().unwrap().id,
                        channel.id,
                        c.id,
                    )),
                    callback_game: None,
                    login_url: None,
                    pay: None,
                    switch_inline_query: None,
                    switch_inline_query_current_chat: None,
                    url: None,
                },
                InlineKeyboardButton {
                    text: "Refuse \u{274c}".to_owned(),
                    callback_data: Some("\u{274c}".to_string()),
                    callback_game: None,
                    login_url: None,
                    pay: None,
                    switch_inline_query: None,
                    switch_inline_query_current_chat: None,
                    url: None,
                },
            ]];
            reply.set_parse_mode(&ParseMode::MarkdownV2);
            reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
                inline_keyboard,
            }));
            ctx.api.send_message(reply).await?;

        // Bot generated url: generate invite url for current user
        } else if user.is_none() && channel.is_some() && c.is_some() {
            let chan = channel.unwrap();
            let c = c.unwrap();
            let bot_name = {
                let guard = ctx.data.read();
                guard
                    .get::<NameKey>()
                    .expect("name")
                    .clone()
                    .replace('@', "")
            };
            let params = BASE64URL.encode(
                format!(
                    "chan={}&contest={}&source={}",
                    chan.id,
                    c.id,
                    message.from.unwrap().id
                )
                .as_bytes(),
            );
            let invite_link = format!(
                "https://t.me/{bot_name}?start={params}"
            );

            let text = &escape_markdown(
                &format!(
                    "Thank you for joining the {contest_name} contest!\n\
            Here's the link to use for inviting your friends to join {chan_name}:\n\n\
            \u{1f449}\u{1f3fb}{invite_link}",
                    contest_name = c.name,
                    chan_name = chan.name,
                    invite_link = invite_link
                ),
                None,
            );
            let mut reply = SendMessage::new(sender_id, text);
            reply.set_parse_mode(&ParseMode::MarkdownV2);
            ctx.api.send_message(reply).await?;
        }
    } else {
        // Case in which no parameter are present

        // If this is a start from a group/supergroup, then we can register the group
        // as a channel. If instead is a start from inside the bot chat, we just say hello.
        let chat_id = message.chat.get_id();
        let registered = channels::try_register(&ctx, chat_id, sender_id).await;
        if registered {
            display_main_commands(&ctx, sender_id).await;
        } else {
            ctx
                .api
                .send_message(SendMessage::new(
                    sender_id,
                    "Welcome to RaF (Refer a Friend) Bot! Have a look at the command list, with /help",
                ))
                .await?;
        }
    }

    info!("start command end");
    Ok(())
}

/// Register command. Shows to the user the procedure to register a channel/group to `RaF`.
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `message` - Received message with the commands inside
///
/// # Panics
/// If telegram servers return error.
#[command(description = "Register your group/channel to the bot")]
pub async fn register(ctx: Context, message: Message) -> CommandResult {
    info!("register command begin");
    let sender_id = message.from.clone().unwrap().id;
    ctx.api
        .send_message(SendMessage::new(
            sender_id,
            "To register a channel to RaF\n\n\
            1) Add the bot as admin in your channel\n\
            2) Forward a message from your channel to complete the registartion\n\n\
            To register a group/supergroup to RaF:\n\n\
            1) Add the bot as admin in your group/supergroup\n\
            2) Start the bot inside the group/supergroup\n\n\
            That's it.",
        ))
        .await?;
    display_main_commands(&ctx, sender_id).await;
    info!("register command end");
    Ok(())
}

/// List command. Shows to the user the channels/groups registered
///
/// # Arguments
/// * `ctx` - Telexide context
/// * `message` - Received message with the commands inside
///
/// # Panics
/// Panics if the connection to the db fails, or if telegram servers return error.
#[command(description = "List your registered channels/groups")]
pub async fn list(ctx: Context, message: Message) -> CommandResult {
    info!("list command begin");
    let sender_id = message.from.clone().unwrap().id;
    let text = {
        let channels = channels::get_all(&ctx, sender_id);

        let mut text: String = String::new();
        for (i, chan) in channels.iter().enumerate() {
            text += &format!(
                "{} [{}]({})\n",
                escape_markdown(&format!("{}.", i + 1), None),
                escape_markdown(&chan.name, None),
                chan.link
            );
        }
        if text.is_empty() {
            escape_markdown("You don't have any channel registered, yet!", None)
        } else {
            text
        }
    };

    let mut reply = SendMessage::new(sender_id, &text);
    reply.set_parse_mode(&ParseMode::MarkdownV2);

    let res = ctx.api.send_message(reply).await;

    if res.is_err() {
        let err = res.err().unwrap();
        error!("[list channels] {}", err);
    }
    display_main_commands(&ctx, sender_id).await;

    info!("list command exit");
    Ok(())
}
