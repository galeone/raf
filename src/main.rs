use chrono::DateTime;

use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;

use std::collections::HashMap;
use std::env;

use telexide::framework::{CommandError, CommandResult};
use telexide::model::{
    Chat, ChatMember, InlineKeyboardButton, InlineKeyboardMarkup, ParseMode, ReplyMarkup,
    UpdateContent,
};
use telexide::{api::types::*, prelude::*};

use typemap::Key;

use data_encoding::BASE64URL;

use log::{error, info};
use simple_logger::SimpleLogger;

#[derive(Debug)]
struct User {
    id: i64,
    name: String,
}
#[derive(Debug)]
struct Channel {
    id: i64,
    registered_by: i64,
    link: String,
    name: String,
}

#[derive(Debug)]
struct Invite {
    id: i64,
    date: DateTime<chrono::Utc>,
    source: i64,
    dest: i64,
    chan: i64,
}

#[command(description = "Help menu")]
async fn help(context: Context, message: Message) -> CommandResult {
    info!("help command begin");
    let text = escape_markdown(
        "I can create contests based on the referral strategy.\
        The user that referes more (legit) users will win a price!\n\n\
        You can control me by sending these commands:\n\n\
        /register - register a channel to the bot\n\
        /list - list your registered channels\n\
        /help - this menu",
        None,
    );
    let mut reply = SendMessage::new(message.chat.get_id(), &text);
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let res = context.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[help] error: {}", err);
    }

    info!("help command end");
    Ok(())
}

#[command(description = "Start/Manage the referral campaign")]
async fn campaign(context: Context, message: Message) -> CommandResult {
    info!("campaign command begin");
    let channels = get_channels(&context, &message);

    let mut reply = SendMessage::new(
        message.chat.get_id(),
        "Select the channel you want to manage",
    );

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
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
        inline_keyboard,
    }));

    let res = context.api.send_message(reply).await;

    if res.is_err() {
        let err = res.err().unwrap();
        error!("[list channels] error: {}", err);
    }

    info!("campaing command end");
    Ok(())
}

#[command(description = "Start the Bot")]
async fn start(context: Context, message: Message) -> CommandResult {
    info!("start command begin");
    // We should also check that at that time the user is not inside the chan
    // and that it comes to the channel only by following this custom link
    // with all the process (referred -> what channel? -> click in @channel
    // (directly from the bot, hence save the chan name) -> joined
    // Once done, check if it's inside (and save the date).

    // On start, save the user ID if not already present
    let res = {
        let guard = context.data.read();
        let map = guard.get::<HashMapKey>().expect("hashmap");
        let conn = map.get().unwrap();
        let user = message.from.clone().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO users(id, name) VALUES(?, ?)",
            params![user.id, user.first_name],
        )
    };
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[insert user] error: {}", err);
        context
            .api
            .send_message(SendMessage::new(
                message.chat.get_id(),
                &format!("[insert user] error: {}", err),
            ))
            .await?;
    }

    // /start?base64encode(source=<uid>&chan=<chan id>)
    // message = "start base64encode(source=ecc)"
    let text = message.get_text().unwrap();
    info!("Start text: {}", text);
    let mut split = text.split_ascii_whitespace();
    split.next(); // /start
    if let Some(encoded_params) = split.next() {
        info!("start params encoded: {}", encoded_params);
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

        if -1 == source || -1 == chan {
            context
                .api
                .send_message(SendMessage::new(
                    message.chat.get_id(),
                    "Invalid /start command parameters.",
                ))
                .await?;
        }
        let (user, channel) = {
            let guard = context.data.read();
            let map = guard.get::<HashMapKey>().expect("hashmap");
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
                .map(|chan| chan.unwrap())
                .next();
            if channel.is_none() {
                (None, None)
            } else {
                let channel = channel.unwrap();

                let mut stmt = conn.prepare("SELECT name FROM users WHERE id = ?").unwrap();

                let user = stmt
                    .query_map(params![source], |row| {
                        Ok(User {
                            id: source,
                            name: row.get(0)?,
                        })
                    })
                    .unwrap()
                    .map(|user| user.unwrap())
                    .next();
                if user.is_none() {
                    (None, None)
                } else {
                    let user = user.unwrap();
                    (Some(user), Some(channel))
                }
            }
        };

        if user.is_none() || channel.is_none() {
            context
                .api
                .send_message(SendMessage::new(
                    message.chat.get_id(),
                    "Something wrong with the channel or the user that's inviting you.\n\
                    Contact the support.",
                ))
                .await?;
            return Err(CommandError(
                "Something wrong with the channel or the user that's inviting you".to_owned(),
            ));
        }
        let user = user.unwrap();
        let channel = channel.unwrap();

        let mut reply = SendMessage::new(
            message.chat.get_id(),
            &format!("{} invited you to join {}", user.name, channel.name),
        );

        let inline_keyboard = vec![vec![
            InlineKeyboardButton {
                text: "Accept ✅".to_owned(),
                // tick, source, dest, chan
                callback_data: Some(format!(
                    "✅ {} {} {}",
                    user.id,
                    message.from.clone().unwrap().id,
                    channel.id
                )),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "Refuse ❌".to_owned(),
                callback_data: Some("❌".to_string()),
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
        context.api.send_message(reply).await?;
    } else {
        // Case in which no parameter are present
        context
            .api
            .send_message(SendMessage::new(
                message.chat.get_id(),
                "Welcome to RaF (Refer a Friend) Bot! Have a look at the command list, with /help",
            ))
            .await?;
    }

    info!("start command exit");
    Ok(())
}

#[command(description = "Register your channel to the bot")]
async fn register(context: Context, message: Message) -> CommandResult {
    info!("register command begin");
    context
        .api
        .send_message(SendMessage::new(
            message.chat.get_id(),
            "Welcome! To Register your channel to RaF you need to:\n\n\
            1) Add the bot as admin in your channel.\n\
            2) Forward a message from your channel to complete the registartion.",
        ))
        .await?;

    info!("register command exit");
    Ok(())
}

fn escape_markdown(text: &str, entity_type: Option<&str>) -> String {
    let mut pattern = r#"'_*[]()~`>#+-=|{}.!"#;
    if let Some(entity) = entity_type {
        pattern = match entity {
            "pre" | "code" => r#"\`"#,
            "text_link" => r#"\)"#,
            _ => pattern,
        };
    }
    let pattern = format!("([{}])", regex::escape(pattern));
    let re = regex::Regex::new(&pattern).unwrap();
    return re.replace_all(text, r#"\$1"#).to_string();
}

fn get_channels(context: &Context, message: &Message) -> Vec<Channel> {
    let guard = context.data.read();
    let map = guard.get::<HashMapKey>().expect("hashmap");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare("SELECT id, link, name FROM channels WHERE registered_by = ?")
        .unwrap();

    let user_id = message.from.clone().unwrap().id;
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
        .map(|chan| chan.unwrap())
        .collect();
    channels
}

#[command(description = "List your registered channels")]
async fn list(context: Context, message: Message) -> CommandResult {
    info!("list command begin");
    let text = {
        let channels = get_channels(&context, &message);

        let mut text: String = "".to_owned();
        for (i, chan) in channels.iter().enumerate() {
            text += &format!(
                "{} [{}]({})\n",
                escape_markdown(&format!("{})", i), None),
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

    let mut reply = SendMessage::new(message.chat.get_id(), &text);
    reply.set_parse_mode(&ParseMode::MarkdownV2);

    let res = context.api.send_message(reply).await;

    if res.is_err() {
        let err = res.err().unwrap();
        error!("[list channels] error: {}", err);
    }

    info!("list command exit");
    Ok(())
}

#[prepare_listener]
async fn callback_handler(context: Context, update: Update) {
    info!("callback handler begin");
    let callback = match update.content {
        UpdateContent::CallbackQuery(ref q) => q,
        _ => return,
    };
    let data = callback.data.clone().unwrap_or_else(|| "".to_string());
    let mut source: i64 = 0;
    let mut dest: i64 = 0;
    let chan_id: i64;
    // Accepted invitation
    let mut accepted = false;
    let mut manage = false;
    let (mut delete, mut stop, mut start) = (false, false, false);
    if data.contains('✅') {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // tick
        source = iter.next().unwrap().parse().unwrap(); // source user
        dest = iter.next().unwrap().parse().unwrap(); // dest user
        chan_id = iter.next().unwrap().parse().unwrap(); // channel id
        accepted = true;
    } else if data.contains('❌') {
        // Rejected invitation
        let text = Some("Ok, doing nothing.".to_string());
        let res = context
            .api
            .answer_callback_query(AnswerCallbackQuery {
                callback_query_id: callback.id.clone(),
                cache_time: None,
                show_alert: false,
                text,
                url: None,
            })
            .await;
        if res.is_err() {
            error!("Callback handler: {}", res.err().unwrap());
        }
        return;
    } else if data.contains("manage") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // manage
        chan_id = iter.next().unwrap().parse().unwrap();
        manage = true;
    } else if data.contains("delete") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // delete
        chan_id = iter.next().unwrap().parse().unwrap();
        delete = true;
        // TODO
    } else if data.contains("stop") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // stop
        chan_id = iter.next().unwrap().parse().unwrap();
        stop = true;
        // TODO
    } else if data.contains("start") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // start
        chan_id = iter.next().unwrap().parse().unwrap();
        start = true;
        // TODO
    } else {
        // Anyway, on no-sense command reply with the empty message
        // to remove the loading icon next to the button
        let res = context
            .api
            .answer_callback_query(AnswerCallbackQuery {
                callback_query_id: callback.id.clone(),
                cache_time: None,
                show_alert: false,
                text: None,
                url: None,
            })
            .await;
        if res.is_err() {
            error!("Callback handler: {}", res.err().unwrap());
        }
        return;
    }

    let message = callback.message.clone().unwrap();
    let user = callback.from.id;

    let chan = {
        let guard = context.data.read();
        let map = guard.get::<HashMapKey>().expect("hashmap");
        let conn = map.get().unwrap();

        let mut stmt = conn
            .prepare("SELECT link, name, registered_by FROM channels WHERE id = ?")
            .unwrap();

        let channel = stmt
            .query_map(params![chan_id], |row| {
                Ok(Channel {
                    id: chan_id,
                    link: row.get(0)?,
                    name: row.get(1)?,
                    registered_by: row.get(2)?,
                })
            })
            .unwrap()
            .map(|chan| chan.unwrap())
            .next()
            .unwrap();
        Some(channel)
    };

    if chan.is_none() {
        return;
    }
    let chan = chan.unwrap();

    if accepted {
        /*
        let member = context
            .api
            .get_chat_member(GetChatMember {
                chat_id: chan.id,
                user_id: invite.dest,
            })
            .await;
        if member.is_ok() {
            let text = format!(
                "You are already a member of [{}]({})\\.",
                escape_markdown(&chan.name.to_string(), None),
                chan.link
            );
            let mut reply = SendMessage::new(message.chat.get_id(), &text);
            reply.set_parse_mode(&ParseMode::MarkdownV2);
            let res = context.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[help] error: {}", err);
            }
            return;
        }
        */
        let res = context
            .api
            .answer_callback_query(AnswerCallbackQuery {
                callback_query_id: callback.id.clone(),
                cache_time: None,
                show_alert: false,
                text: None,
                url: None,
            })
            .await;
        if res.is_err() {
            error!("Callback handler: {}", res.err().unwrap());
        }
        let text = format!(
            "Please join 👉 [{}]({}) within the next 15 seconds\\.",
            escape_markdown(&chan.name.to_string(), None),
            chan.link
        );
        let mut reply = SendMessage::new(message.chat.get_id(), &text);
        reply.set_parse_mode(&ParseMode::MarkdownV2);
        let res = context.api.send_message(reply).await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[please join] error: {}", err);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(15)).await;
        let member = context
            .api
            .get_chat_member(GetChatMember {
                chat_id: chan.id,
                user_id: user,
            })
            .await;

        let joined = member.is_ok();
        if !joined {
            info!("User not joined the channel after 15 seconds...");
        } else {
            info!("Refer OK!");
            let res = {
                let guard = context.data.read();
                let map = guard.get::<HashMapKey>().expect("hashmap");
                let conn = map.get().unwrap();
                conn.execute(
                    "INSERT INTO invitations(source, dest, chan) VALUES(?, ?, ?)",
                    params![source, dest, chan.id],
                )
            };
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[insert invitation] error: {}", err);
                let res = context
                    .api
                    .send_message(SendMessage::new(
                        message.chat.get_id(),
                        "Failed to insert invitation: this invitation might already exist!",
                    ))
                    .await;
                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[failed to insert invitation] error: {}", err);
                }
                return;
            }
            let text = format!(
                "You joined [{}]({}) 🤗",
                escape_markdown(&chan.name.to_string(), None),
                chan.link
            );
            let mut reply = SendMessage::new(message.chat.get_id(), &text);
            reply.set_parse_mode(&ParseMode::MarkdownV2);
            let res = context.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[help] error: {}", err);
            }
        }
    }

    if manage {
        let mut reply = SendMessage::new(message.chat.get_id(), "What do you want to do?");
        reply.set_parse_mode(&ParseMode::MarkdownV2);
        let inline_keyboard = vec![vec![
            InlineKeyboardButton {
                text: "Start a campagin".to_owned(),
                // start, chan
                callback_data: Some(format!(
                    "start {}",
                    chan.id
                )),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "Stop a campagin".to_owned(),
                callback_data: Some(format!("stop {}", chan.id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "Delete a campagin".to_owned(),
                callback_data: Some(format!("delete {}", chan.id)),
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

        let res = context.api.send_message(reply).await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[manage send] error: {}", err);
        }
        // required to remove the loading icon next to the button
        let res = context
            .api
            .answer_callback_query(AnswerCallbackQuery {
                callback_query_id: callback.id.clone(),
                cache_time: None,
                show_alert: false,
                text: None,
                url: None,
            })
            .await;
        if res.is_err() {
            error!("Callback handler: {}", res.err().unwrap());
        }
    }

    if start {
        // TODO
    }
    
    if stop {
        // TODO
    }

    if delete {
        // TODO
    }
}

#[prepare_listener]
async fn message_handler(context: Context, update: Update) {
    info!("message handler begin");
    let message = match update.content {
        UpdateContent::Message(ref m) => m,
        _ => return,
    };

    // If the user if forwarding a message from a channel, we
    // are in the registration flow.
    let mut chat: Option<&telexide::model::ChannelChat> = None;
    if let Some(forward_data) = &message.forward_data {
        if let Some(from_chat) = &forward_data.from_chat {
            chat = match from_chat {
                Chat::Channel(c) => Some(c),
                _ => {
                    let res = context
                        .api
                        .send_message(SendMessage::new(
                            message.chat.get_id(),
                            "Error: forward a message from a channel.",
                        ))
                        .await;
                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[message handler] send message error: {}", err);
                    }
                    return;
                }
            };
        }
    }

    let registration_flow = chat.is_some();

    if registration_flow {
        let chat = chat.unwrap();
        let registered_by = message.from.clone().unwrap().id;

        // NOTE: we need this get_chat call because chat.invite_link is returned only by
        // calling GetChat: https://core.telegram.org/bots/api#chat
        let chat = match context
            .api
            .get_chat(GetChat { chat_id: chat.id })
            .await
            .unwrap()
        {
            Chat::Channel(c) => c,
            _ => return,
        };

        let link: String = {
            if let Some(invite_link) = chat.invite_link {
                invite_link.to_string()
            } else if let Some(username) = chat.username {
                format!("https://t.me/{}", username)
            } else {
                "".to_owned()
            }
        };

        if link.is_empty() {
            error!("Unable to extract invite link / username for {}", chat.id);
            return;
        }

        let admins = context
            .api
            .get_chat_administrators(GetChatAdministrators { chat_id: chat.id })
            .await;
        if admins.is_err() {
            let res = context
                .api
                .send_message(SendMessage::new(
                    message.chat.get_id(),
                    "You must first add the bot as admin of the channel!",
                ))
                .await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[message handler] send message (loop admin) error: {}", err);
            }
            return;
        }
        let admins = admins.unwrap();
        let me = context.api.get_me().await.unwrap();
        let mut found = false;
        for admin in admins {
            let admin = match admin {
                ChatMember::Administrator(admin_member_status) => admin_member_status,
                _ => {
                    let res = context
                        .api
                        .send_message(SendMessage::new(
                            message.chat.get_id(),
                            "You have a not admin member in your admin list (how?)",
                        ))
                        .await;
                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[message handler] send message (loop admin) error: {}", err);
                    }
                    return;
                }
            };
            if admin.user.is_bot
                && admin.user.id == me.id
                && admin.can_manage_chat
                && (admin.can_post_messages.is_some() && admin.can_post_messages.unwrap())
            {
                found = true;
                break;
            }
        }

        if !found {
            let res = context
                .api
                .send_message(SendMessage::new(
                    message.chat.get_id(),
                    "The bot must be admin of the channel, and shall be able to manage the chat.",
                ))
                .await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[message handler] send message (loop admin) error: {}", err);
            }
            return;
        }

        let res = {
            let guard = context.data.read();
            let map = guard.get::<HashMapKey>().expect("hashmap");
            let conn = map.get().unwrap();

            conn.execute(
                "INSERT OR IGNORE INTO channels(id, registered_by, link, name) VALUES(?, ?, ?, ?)",
                params![chat.id, registered_by, link, chat.title],
            )
        };

        if res.is_err() {
            let err = res.err().unwrap();
            error!("[message handler] error: {}", err);

            let res = context
                .api
                .send_message(SendMessage::new(
                    message.chat.get_id(),
                    "Forward a message from a channel.",
                ))
                .await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[message handler] send message error: {}", err);
            }
            return;
        }

        let res = context
            .api
            .send_message(SendMessage::new(
                message.chat.get_id(),
                &format!("Channel {} registered succesfully!", chat.title),
            ))
            .await;

        if res.is_err() {
            let err = res.err().unwrap();
            error!("[message handler] final send message error: {}", err);
        }
    } else {
        // If we are not in the registartion flow, we just received a message
        // and we should check if the message is among the accepted ones.

        let text = message.get_text().unwrap();
        // We also receive commands in this handler, we need to skip them
        if text.starts_with('/') {
            return;
        }
    }

    info!("message handler exit");
}

fn init_db() -> r2d2::Pool<SqliteConnectionManager> {
    let manager = SqliteConnectionManager::file("raf.db")
        .with_init(|c| c.execute_batch("PRAGMA foreign_keys=1;"));
    let pool = r2d2::Pool::builder().max_size(15).build(manager).unwrap();
    {
        let conn = pool.get().unwrap();
        conn.execute_batch(
            "BEGIN;
            CREATE TABLE IF NOT EXISTS users (
               id   INTEGER PRIMARY KEY NOT NULL,
               name TEXT NOT NULL
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
               date TIMESTAP DEFAULT CURRENT_TIMESTAMP NOT NULL,
               source INTEGER NOT NULL,
               dest INTEGER NOT NULL,
               chan INTEGER NOT NULL,
               FOREIGN KEY(source) REFERENCES users(id),
               FOREIGN KEY(dest) REFERENCES users(id),
               FOREIGN KEY(chan) REFERENCES channels(id),
               UNIQUE(source, dest, chan)
            );
            COMMIT;",
        )
        .unwrap();
    }
    pool
}

struct HashMapKey;
impl Key for HashMapKey {
    type Value = r2d2::Pool<SqliteConnectionManager>;
}

use log::LevelFilter;
#[tokio::main]
async fn main() -> telexide::Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .init()
        .unwrap();

    let pool = init_db();
    let token = env::var("TOKEN").expect("Provide the token via TOKEN env var");

    let client = ClientBuilder::new()
        .set_token(&token)
        .set_framework(create_framework!(
            "test_test_testa_bot",
            help,
            start,
            register,
            campaign,
            list
        ))
        .set_allowed_updates(vec![UpdateType::CallbackQuery, UpdateType::Message])
        .add_handler_func(message_handler)
        .add_handler_func(callback_handler)
        .build();

    {
        let mut data = client.data.write();
        data.insert::<HashMapKey>(pool);
    }
    client.start().await.expect("WAT");
    Ok(())
}
