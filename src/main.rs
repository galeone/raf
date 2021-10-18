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

use tabular::{Row, Table};

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
struct BeingManagedChannel {
    chan: i64,
}

#[derive(Debug)]
struct Invite {
    id: i64,
    date: DateTime<chrono::Utc>,
    source: i64,
    dest: i64,
    chan: i64,
}

#[derive(Debug)]
struct Campaign {
    id: i64,
    name: String,
    prize: String,
    start: DateTime<chrono::Utc>,
    end: DateTime<chrono::Utc>,
    chan: i64,
}

async fn send_main_commands_message(context: Context, message: &Message) {
    let text = escape_markdown(
        "What do you want to do?\n\
        /register - Register a channel to the bot\n\
        /list - List your registered channels\n\
        /campaign - Start/Manage the referral campaign\n",
        None,
    );
    let mut reply = SendMessage::new(message.chat.get_id(), &text);
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let res = context.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[help] error: {}", err);
    }

}

#[command(description = "Help menu")]
async fn help(context: Context, message: Message) -> CommandResult {
    info!("help command begin");
    let text = escape_markdown(
        "I can create contests based on the referral strategy.\
        The user that referes more (legit) users will win a prize!\n\n\
        You can control me by sending these commands:\n\n\
        /register - Register a channel to the bot\n\
        /list - List your registered channels\n\
        /campaign - Start/Manage the referral campaign\n\
        /help - This menu",
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
    reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
        inline_keyboard,
    }));
    reply.set_parse_mode(&ParseMode::MarkdownV2);

    let res = context.api.send_message(reply).await;

    if res.is_err() {
        let err = res.err().unwrap();
        error!("[list channels] error: {}", err);
    }

    info!("campaign command end");
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
            "To Register your channel to RaF please:\n\n\
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
        .prepare("SELECT id, link, name FROM channels WHERE registered_by = ? ORDER BY id DESC")
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

fn get_campaigns(context: &Context, chan: i64) -> Vec<Campaign> {
    let guard = context.data.read();
    let map = guard.get::<HashMapKey>().expect("hashmap");
    let conn = map.get().unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT id, name, prize, start, end FROM campaigns WHERE chan = ? ORDER BY end DESC",
        )
        .unwrap();

    let campaigns = stmt
        .query_map(params![chan], |row| {
            Ok(Campaign {
                id: row.get(0)?,
                name: row.get(1)?,
                prize: row.get(2)?,
                start: row.get(3)?,
                end: row.get(4)?,
                chan,
            })
        })
        .unwrap()
        .map(|campaign| campaign.unwrap())
        .collect();
    campaigns
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
    send_main_commands_message(context, &message).await;

    info!("list command exit");
    Ok(())
}

async fn display_manage_menu(context: &Context, message: &Message, chan_id: i64) {
    let mut reply = SendMessage::new(message.chat.get_id(), "What do you want to do?");
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    let inline_keyboard = vec![
        vec![
            InlineKeyboardButton {
                text: "Create".to_owned(),
                // start, chan
                callback_data: Some(format!("create {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "Delete".to_owned(),
                callback_data: Some(format!("delete {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
        ],
        vec![
            InlineKeyboardButton {
                text: "Start".to_owned(),
                // start, chan
                callback_data: Some(format!("start {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
            InlineKeyboardButton {
                text: "Stop".to_owned(),
                callback_data: Some(format!("stop {}", chan_id)),
                callback_game: None,
                login_url: None,
                pay: None,
                switch_inline_query: None,
                switch_inline_query_current_chat: None,
                url: None,
            },
        ],
        vec![InlineKeyboardButton {
            text: "List".to_owned(),
            callback_data: Some(format!("list {}", chan_id)),
            callback_game: None,
            login_url: None,
            pay: None,
            switch_inline_query: None,
            switch_inline_query_current_chat: None,
            url: None,
        }],
    ];
    reply.set_parse_mode(&ParseMode::MarkdownV2);
    reply.set_reply_markup(&ReplyMarkup::InlineKeyboardMarkup(InlineKeyboardMarkup {
        inline_keyboard,
    }));

    let res = context.api.send_message(reply).await;
    if res.is_err() {
        let err = res.err().unwrap();
        error!("[manage send] error: {}", err);
    }
}

async fn remove_loading_icon(context: &Context, callback_id: &str, text: Option<&str>) {
    let res = context
        .api
        .answer_callback_query(AnswerCallbackQuery {
            callback_query_id: callback_id.to_string(),
            cache_time: None,
            show_alert: text.is_some(),
            text: if text.is_some() { Some(text.unwrap().to_string()) } else {None},
            url: None,
        })
        .await;
    if res.is_err() {
        error!("Callback handler: {}", res.err().unwrap());
    }
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
    // Manage commands
    let (mut create, mut delete, mut stop, mut start, mut list) =
        (false, false, false, false, false);
    // Delete Campaign commands
    let mut delete_campaign = false;
    let mut campaign_id = 0;
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
    } else if data.starts_with("manage") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // manage
        chan_id = iter.next().unwrap().parse().unwrap();
        manage = true;
    } else if data.starts_with("create") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // delete
        chan_id = iter.next().unwrap().parse().unwrap();
        create = true;
    } else if data.starts_with("delete_campaign") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // delete
        chan_id = iter.next().unwrap().parse().unwrap();
        campaign_id = iter.next().unwrap().parse().unwrap();
        delete_campaign = true;
    } else if data.starts_with("delete") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // delete
        chan_id = iter.next().unwrap().parse().unwrap();
        delete = true;
    } else if data.starts_with("stop") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // stop
        chan_id = iter.next().unwrap().parse().unwrap();
        stop = true;
        // TODO
    } else if data.starts_with("start") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // start
        chan_id = iter.next().unwrap().parse().unwrap();
        start = true;
        // TODO
    } else if data.starts_with("list") {
        let mut iter = data.split_ascii_whitespace();
        iter.next(); // start
        chan_id = iter.next().unwrap().parse().unwrap();
        list = true;
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
         * TODO: uncomment me before release
         * This is a check for already existing users in the channel
         */
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
            "Please join 👉 [{}]({}) within the next 10 seconds\\.",
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

        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
        let member = context
            .api
            .get_chat_member(GetChatMember {
                chat_id: chan.id,
                user_id: user,
            })
            .await;

        let joined = member.is_ok();
        if !joined {
            info!("User not joined the channel after 10 seconds...");
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
        remove_loading_icon(&context, &callback.id, None).await;
        display_manage_menu(&context, &message, chan.id).await;
    }

    if start {
        // TODO
        remove_loading_icon(&context, &callback.id, None).await;
        display_manage_menu(&context, &message, chan.id).await;
    }

    if stop {
        // TODO

        remove_loading_icon(&context, &callback.id, None).await;
        display_manage_menu(&context, &message, chan.id).await;

    }

    if create {
        let mut reply = SendMessage::new(
            message.chat.get_id(),
            &escape_markdown(
                "Write a single message with every required info on a new line\n\n\
                Campagin name\n\
                Start date (YYYY-MM-DD hh:mm TZ)\n\
                End date (YYY-MM-DD hh:mm TZ)\n\
                Prize\n\n\
                For example a valid message is (note the GMT+1 timezone written as +01):\n\n\
                August\n\
                2021-08-01 13:00 +01\n\
                2021-08-31 20:00 +01\n\
                Amazon 50€ Gift Card\n",
                None,
            ),
        );
        reply.set_parse_mode(&ParseMode::MarkdownV2);

        let res = context.api.send_message(reply).await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[create send] error: {}", err);
        }

        // adding chan to being_managed_channels since the raw
        // reply falls outiside this FSM
        let res = {
            let guard = context.data.read();
            let map = guard.get::<HashMapKey>().expect("hashmap");
            let conn = map.get().unwrap();
            conn.execute(
                "INSERT INTO being_managed_channels(chan) VALUES(?)",
                params![chan.id],
            )
        };

        if res.is_err() {
            let err = res.err().unwrap();
            error!("[insert being_managed_channels] error: {}", err);
        }

        remove_loading_icon(&context, &callback.id, None).await;
    }

    if delete {
        let campaigns = get_campaigns(&context, chan.id);
        if campaigns.is_empty() {
            remove_loading_icon(&context, &callback.id, Some("You have no campaigns to delete!")).await;
        } else {
            let mut reply = SendMessage::new(
                message.chat.get_id(),
                &escape_markdown("Select the campaign to delete", None),
            );
            let mut partition_size: usize = campaigns.len() / 2;
            if partition_size < 2 {
                partition_size = 1;
            }
            let inline_keyboard: Vec<Vec<InlineKeyboardButton>> = campaigns
                .chunks(partition_size)
                .map(|chunk| {
                    chunk
                        .iter()
                        .map(|campaign| InlineKeyboardButton {
                            text: campaign.name.clone(),
                            // delete_campaing, channel id, campaing id
                            callback_data: Some(format!(
                                "delete_campaign {} {}",
                                chan.id, campaign.id
                            )),
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

            let res = context.api.send_message(reply).await;
            if res.is_err() {
                let err = res.err().unwrap();
                error!("[create send] error: {}", err);
            }
            remove_loading_icon(&context, &callback.id, None).await;
        };
    }

    if list {
        let text = {
            let campaigns = get_campaigns(&context, chan.id);
            let mut text: String = "".to_string();
            if !campaigns.is_empty() {
                text += "```\n";
                let mut table = Table::new("{:<} | {:<} | {:<} | {:<}");
                table.add_row(
                    Row::new()
                        .with_cell("Name")
                        .with_cell("Start")
                        .with_cell("End")
                        .with_cell("Prize"),
                );
                for (_, campaign) in campaigns.iter().enumerate() {
                    table.add_row(
                        Row::new()
                            .with_cell(&campaign.name)
                            .with_cell(campaign.start)
                            .with_cell(campaign.end)
                            .with_cell(&campaign.prize),
                    );
                }
                text += &format!("{}```\n\n{}", table, escape_markdown("Dates are all converted to UTC timezone.\nThis table is better on Desktop.", None));
            }
            text
        };

        if !text.is_empty() {
            let mut reply = SendMessage::new(message.chat.get_id(), &text);
            reply.set_parse_mode(&ParseMode::MarkdownV2);

            let res = context.api.send_message(reply).await;

            if res.is_err() {
                let err = res.err().unwrap();
                error!("[list campaigns] error: {}", err);
            }
            remove_loading_icon(&context, &callback.id, None).await;
            display_manage_menu(&context, &message, chan.id).await;
        } else {
            remove_loading_icon(&context, &callback.id, Some("You don't have any active or past campaigns for this channel!")).await;
        }
    }

    if delete_campaign {
        let res = {
            let guard = context.data.read();
            let map = guard.get::<HashMapKey>().expect("hashmap");
            let conn = map.get().unwrap();
            let mut stmt = conn.prepare("DELETE FROM campaigns WHERE id = ?").unwrap();
            stmt.execute(params![campaign_id])
        };
        let text = if res.is_err() {
            let err = res.unwrap_err();
            error!("delete from campaigns: {}", err);
            err.to_string()
        } else {
            "Done!".to_string()
        };
        let res = context
            .api
            .send_message(SendMessage::new(
                message.chat.get_id(),
                &text
            ))
            .await;
        if res.is_err() {
            let err = res.err().unwrap();
            error!("[message handler] send message (loop admin) error: {}", err);
        }

        remove_loading_icon(&context, &callback.id, None).await;
        display_manage_menu(&context, &message, chan.id).await;
    }
}

fn campagin_from_text(text: &str, chan: i64) -> Option<Campaign> {
    let rows = text
        .split('\n')
        .skip_while(|r| r.is_empty())
        .collect::<Vec<&str>>();
    if rows.len() != 4 {
        error!("failed because row.len() != 4. Got: {}", rows.len());
        return None;
    }
    let id = -1;
    let name = rows[0].to_string();
    let prize = rows[3].to_string();
    // user input: YYYY-MM-DD hh:mm TZ, needs to become
    // YYYY-MM-DD hh:mm:ss TZ to get enough data to create a datetime object
    let add_seconds = |row: &str| -> String {
        let mut elements = row
            .split_whitespace()
            .map(|x| x.to_string())
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
    let start = DateTime::parse_from_str(&add_seconds(rows[1]), "%Y-%m-%d %H:%M:%S %#z");
    if start.is_err() {
        info!(
            "failed because start.is_err: {}: {}",
            start.unwrap_err(),
            add_seconds(rows[1])
        );
        return None;
    }
    let start: DateTime<chrono::Utc> = start.unwrap().into();
    let end = DateTime::parse_from_str(&add_seconds(rows[2]), "%Y-%m-%d %H:%M:%S %#z");
    if end.is_err() {
        info!("failed because end.is_err: {}", end.unwrap_err());
        return None;
    }
    let end: DateTime<chrono::Utc> = end.unwrap().into();
    Some(Campaign {
        id,
        start,
        end,
        name,
        prize,
        chan,
    })
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
                            "You have a non-admin member in your admin list (how?)",
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
        send_main_commands_message(context, message).await;
    } else {
        // If we are not in the registartion flow, we just received a message
        // and we should check if the message is among the accepted ones.

        let text = message.get_text().unwrap();
        // We also receive commands in this handler, we need to skip them
        if text.starts_with('/') {
            return;
        }

        // Check if some of the user channel's are being managed
        // in that case it's plausible that the user is sending the message in this format
        // ```
        // campaign name
        // start date (YYYY-MM-DD hh:mm TZ)
        // end date (YYYY-MM-DD hh:mm TZ)
        // prize
        // ```
        let channels = get_channels(&context, message); // channels registered by the user
        let chan = {
            let guard = context.data.read();
            let map = guard.get::<HashMapKey>().expect("hashmap");
            let conn = map.get().unwrap();
            // There should be only one at the time, from the same user, being edited. Hence loop
            // only once.
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT chan FROM being_managed_channels WHERE chan IN ({})",
                    channels
                        .iter()
                        .map(|c| c.id.to_string())
                        .collect::<Vec<String>>()
                        .join(",")
                ))
                .unwrap();
            let chan = stmt
                .query_map(params![], |row| {
                    Ok(BeingManagedChannel { chan: row.get(0)? })
                })
                .unwrap()
                .map(|chan| chan.unwrap())
                .next();
            chan
        };
        if chan.is_some() {
            let chan = chan.unwrap().chan;
            let campagin = campagin_from_text(&text, chan);

            if let Some(campagin) = campagin {
                let res = {
                    let guard = context.data.read();
                    let map = guard.get::<HashMapKey>().expect("hashmap");
                    let conn = map.get().unwrap();
                    conn.execute(
                        "INSERT INTO campaigns(name, start, end, prize, chan)  VALUES(?, ?, ?, ?, ?)",
                        params![campagin.name, campagin.start, campagin.end, campagin.prize, campagin.chan]
                    )
                };
                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[insert campagin] error: {}", err);
                } else {
                    let res = context
                        .api
                        .send_message(SendMessage::new(
                            message.chat.get_id(),
                            &format!("Campaing {} created succesfully!", campagin.name),
                        ))
                        .await;

                    if res.is_err() {
                        let err = res.err().unwrap();
                        error!("[message handler] campaing ok send error: {}", err);
                    }
                }
            } else {
                let res = context
                    .api
                    .send_message(SendMessage::new(
                        message.chat.get_id(),
                        "Something wrong happened while creating your new campagin.\n\n\
                        Please restart the campagin creating process and send a correct message (check the time zone format!)",
                    ))
                    .await;

                if res.is_err() {
                    let err = res.err().unwrap();
                    error!("[message handler] campaing ok send error: {}", err);
                }
            }
            // Delete from the currently managing campagins, since creating is completed
            let res = {
                let guard = context.data.read();
                let map = guard.get::<HashMapKey>().expect("hashmap");
                let conn = map.get().unwrap();
                let mut stmt = conn
                    .prepare("DELETE FROM being_managed_channels WHERE chan = ?")
                    .unwrap();
                stmt.execute(params![chan])
            };
            if res.is_err() {
                error!("delete from being_managed_channels: {}", res.unwrap_err());
            }

            display_manage_menu(&context, &message, chan).await;
        }
        // else, if no channel is being edited, ignore and move on
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
               date TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
               source INTEGER NOT NULL,
               dest INTEGER NOT NULL,
               chan INTEGER NOT NULL,
               FOREIGN KEY(source) REFERENCES users(id),
               FOREIGN KEY(dest) REFERENCES users(id),
               FOREIGN KEY(chan) REFERENCES channels(id),
               UNIQUE(source, dest, chan)
            );
            CREATE TABLE IF NOT EXISTS campaigns(
              id INTEGER PRIMARY KEY AUTOINCREMENT,
              name TEXT NOT NULL,
              prize TEXT NOT NULL,
              start TIMESTAMP NOT NULL,
              end TIMESTAMP NOT NULL,
              chan INTEGER NOT NULL,
              FOREIGN KEY(chan) REFERENCES channels(id),
              UNIQUE(name)
            );
            CREATE TABLE IF NOT EXISTS being_managed_channels(
              chan INTEGER NOT NULL PRIMARY KEY,
              FOREIGN KEY(chan) REFERENCES channels(id)
            );
            COMMIT;",
        )
        .unwrap();
    }

    // being_managed_channels, as the name suggests, is the channel that the owner (
    // hence channels.registered_by == owner) is managing.
    //
    // This table is required because we need a "complex" message, send as plain text
    // with all the required info. But this is outside the FSM created by the buttons
    // (FSM created naturally because all the callbacks invokes the same method).
    //
    // Hence, we don't know by the message alone what was the user currently managing
    // because in the raw text message callback we only have a single info, that's the user
    // sender id.
    // Once received the text message and made the association chan<-> user we must delete
    // the entry in the being_managed_channels table, so the next raw message by this user
    // is not associated with the campaign management phase.

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
